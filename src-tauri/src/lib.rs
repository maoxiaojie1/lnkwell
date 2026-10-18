mod error;
pub mod markdown;
// 集成测试通过 `lnkwell_lib::vault::desktop::resolve_entry_inner` 验证路径解析，
// 因此公开该模块（改进计划 C3）
pub mod vault;

use error::{AppError, AppResult};
use markdown::RenderResult;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::{AppHandle, Manager};
// Emitter 只在桌面端用（单实例插件转发 open-shared-file）
#[cfg(desktop)]
use tauri::Emitter;
use vault::{EntryRef, FileNode, VaultRef};

/// 渲染缓存条目：文件 mtime + 已渲染结果
struct CacheEntry {
    mtime: i64,
    result: RenderResult,
}

/// 进程内渲染缓存（方案 §8.2）：同会话内反复打开同一文档跳过解析。
/// 用 mtime 校验失效；容量上限 32 条，超出清空重建（阅读场景条目有限，LRU 成本不值得）。
static RENDER_CACHE: OnceLock<Mutex<HashMap<String, CacheEntry>>> = OnceLock::new();

fn render_cache() -> &'static Mutex<HashMap<String, CacheEntry>> {
    RENDER_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

const RENDER_CACHE_CAP: usize = 32;

/// 桌面端冷启动时由命令行传入的待打开文件（双击 .md / 系统打开方式）。
/// setup 阶段 webview 尚未加载、emit 事件会丢失，故先暂存，
/// 前端就绪后通过 take_pending_share 命令取走（取走即清）。
static PENDING_SHARE: OnceLock<Mutex<Option<String>>> = OnceLock::new();

fn pending_share() -> &'static Mutex<Option<String>> {
    PENDING_SHARE.get_or_init(|| Mutex::new(None))
}

/// 从启动参数中找出第一个实际存在的 Markdown 文件路径。
/// dev 模式下的各类 flag 因不以 .md 结尾或非文件而自然被过滤。
#[cfg(desktop)]
fn find_shared_file(args: impl IntoIterator<Item = String>) -> Option<String> {
    use std::path::Path;
    args.into_iter()
        .find(|a| vault::is_markdown(a) && Path::new(a).is_file())
}

/// 前端启动时取走暂存的待打开文件路径（桌面双击文件冷启动场景），无则返回 None
#[tauri::command]
fn take_pending_share() -> Option<String> {
    // 锁中毒时继续使用内部值，避免命令路径 panic（改进计划 B3）
    pending_share().lock().unwrap_or_else(|e| e.into_inner()).take()
}

#[derive(Debug, Serialize)]
pub struct VaultInfo {
    pub root: String,
}

/// 选择文档库根目录（方案 §4.2）
#[tauri::command]
async fn pick_vault(app: AppHandle) -> AppResult<Option<String>> {
    let r = vault::pick_root(&app).await;
    match &r {
        Ok(Some(v)) => log::info!("pick_vault ok: {}", &v.0[..v.0.len().min(80)]),
        Ok(None) => log::info!("pick_vault: user cancelled"),
        Err(e) => log::error!("pick_vault err: {e:?}"),
    }
    Ok(r?.map(|v| v.0))
}

/// 校验库访问权限是否仍有效（方案 §11.4）
#[tauri::command]
async fn check_vault_permission(app: AppHandle, root: String) -> AppResult<bool> {
    vault::check_permission(&app, &VaultRef(root)).await
}

/// 列目录（方案 §4.2：过滤隐藏项、仅 md、显式排序）
#[tauri::command]
async fn list_dir(app: AppHandle, dir: String) -> AppResult<Vec<FileNode>> {
    vault::list(&app, &EntryRef(dir)).await
}

/// 读取图片并编码为 data URL，前端直接赋给 img.src（绕开 asset 协议与 IPC 字节传输）
#[tauri::command]
async fn read_image(app: AppHandle, file: String) -> AppResult<String> {
    vault::read_image(&app, &EntryRef(file)).await
}

/// 是否移动端（Android/iOS）。前端据此隐藏桌面专属功能（如导出 PDF）。
#[tauri::command]
fn is_mobile() -> bool {
    cfg!(any(target_os = "android", target_os = "ios"))
}

/// 导出当前文档为 PDF（方案 §7.2）。
/// Windows 上直接调用 WebView2 的 PrintToPdf：先弹保存对话框选定路径，
/// 再对当前 WebView 内容快照生成 PDF，完全绕过打印对话框，
/// 同时通过关闭 ShouldPrintHeaderAndFooter 消除默认页眉页脚里的 localhost:1420。
/// 返回保存路径；用户取消返回 None。
#[tauri::command]
async fn export_pdf(app: AppHandle, suggested_name: Option<String>) -> AppResult<Option<String>> {
    #[cfg(target_os = "windows")]
    {
        export_pdf_windows(&app, suggested_name).await
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (app, suggested_name);
        Err(AppError::InvalidInput("当前平台不支持导出 PDF".into()))
    }
}

#[cfg(target_os = "windows")]
async fn export_pdf_windows(
    app: &AppHandle,
    suggested_name: Option<String>,
) -> AppResult<Option<String>> {
    use tauri_plugin_dialog::DialogExt;

    // 1. 弹保存对话框（blocking API 内部会投递到主线程，安全地从 async 线程调用）
    let mut builder = app.dialog().file().add_filter("PDF", &["pdf"]);
    if let Some(name) = &suggested_name {
        builder = builder.set_file_name(name);
    }
    let Some(path) = builder.blocking_save_file() else {
        return Ok(None); // 用户取消
    };
    let path_buf = path
        .into_path()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let output = path_buf.to_string_lossy().into_owned();

    // 2. 主线程上发起 PrintToPdf，结果经 channel 传回
    let webview = app
        .get_webview_window("main")
        .ok_or_else(|| AppError::InvalidInput("主窗口不存在".into()))?;

    let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();
    let output_for_thread = output.clone();

    // with_webview 的闭包通过事件循环投递到主线程异步执行，立即返回；
    // 闭包在主线程发起 PrintToPdf 后即返回，完成回调再 send 结果到 channel。
    webview
        .with_webview(move |platform| {
            if let Err(e) = print_to_pdf(platform, &output_for_thread, tx.clone()) {
                let _ = tx.send(Err(e));
            }
        })
        .map_err(|e| AppError::InvalidInput(format!("访问 WebView 失败: {e}")))?;

    match rx.recv_timeout(std::time::Duration::from_secs(120)) {
        Ok(Ok(())) => Ok(Some(output)),
        Ok(Err(e)) => Err(AppError::InvalidInput(format!("导出 PDF 失败: {e}"))),
        Err(_) => Err(AppError::InvalidInput("导出 PDF 超时".into())),
    }
}

/// 发起 WebView2 PrintToPdf。同步阶段的错误（COM 调用失败）直接返回，
/// 异步打印完成结果经 `tx` 送回。
#[cfg(target_os = "windows")]
fn print_to_pdf(
    platform: tauri::webview::PlatformWebview,
    output: &str,
    tx: std::sync::mpsc::Sender<Result<(), String>>,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        ICoreWebView2PrintSettings, ICoreWebView2Environment6, ICoreWebView2_7,
        COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT,
    };
    use webview2_com::PrintToPdfCompletedHandler;
    use windows_core::Interface;

    let controller = platform.controller();
    // PrintToPdf 定义在 ICoreWebView2_7，需从基础接口 QueryInterface 升级
    let core = unsafe { controller.CoreWebView2() }.map_err(|e| e.to_string())?;
    let core7: ICoreWebView2_7 = core.cast().map_err(|e| e.to_string())?;

    // 打印设置：A4 纵向、无页眉页脚、不打印背景。
    // CreatePrintSettings 定义在 ICoreWebView2Environment6
    let env = platform.environment();
    let env6: ICoreWebView2Environment6 = env.cast().map_err(|e| e.to_string())?;
    let settings: ICoreWebView2PrintSettings =
        unsafe { env6.CreatePrintSettings() }.map_err(|e| e.to_string())?;

    unsafe {
        // 关键：关闭页眉页脚，消除 Chromium 默认页脚的 URL（localhost:1420）
        settings
            .SetShouldPrintHeaderAndFooter(false)
            .map_err(|e| e.to_string())?;
        // 不打印背景（与浏览器打印默认一致）。
        // 关键：若打印背景，深色主题下根 canvas/body 的炭黑背景会被原样打印，
        // 在正文四周形成黑边——即便前端注入白底样式也无法覆盖（打印的是计算后的根背景）。
        // 代码块/引用/表头的浅底改为用边框呈现，不依赖背景色。
        settings
            .SetShouldPrintBackgrounds(false)
            .map_err(|e| e.to_string())?;
        settings
            .SetOrientation(COREWEBVIEW2_PRINT_ORIENTATION_PORTRAIT)
            .map_err(|e| e.to_string())?;
        // A4：210mm × 297mm（英寸）
        settings.SetPageWidth(8.27).map_err(|e| e.to_string())?;
        settings.SetPageHeight(11.69).map_err(|e| e.to_string())?;
        settings.SetScaleFactor(1.0).map_err(|e| e.to_string())?;
        // 页边距（英寸）：与参考脚本 render_pdf.mjs 的 page.pdf margin 一致，
        // 四周均 2cm（2/2.54≈0.7874 inch），保证内容宽度一致、页数不虚增。
        settings.SetMarginTop(0.7874).map_err(|e| e.to_string())?;
        settings.SetMarginBottom(0.7874).map_err(|e| e.to_string())?;
        settings.SetMarginLeft(0.7874).map_err(|e| e.to_string())?;
        settings.SetMarginRight(0.7874).map_err(|e| e.to_string())?;
    }

    let hpath = windows_core::HSTRING::from(output);

    // 完成回调：把结果送回 channel（在 WebView2 的 UI 线程触发，跨线程安全）
    let callback_tx = tx.clone();
    let callback = PrintToPdfCompletedHandler::create(Box::new(
        move |err: windows_core::Result<()>, _is_success: bool| {
            let _ = callback_tx.send(err.map_err(|e| e.to_string()));
            Ok(())
        },
    ));

    unsafe { core7.PrintToPdf(&hpath, &settings, &callback) }.map_err(|e| e.to_string())?;

    Ok(())
}

/// 读取并渲染文档（方案 §5.1 数据流）。带进程内渲染缓存（方案 §8.2）：
/// 以 entry + mtime 校验，命中直接返回，跳过 pulldown-cmark 解析。
#[tauri::command]
async fn read_doc(app: AppHandle, entry: String) -> AppResult<RenderResult> {
    let entry_ref = EntryRef(entry.clone());

    // mtime 校验（桌面真实值；Android SAF 无 mtime，返回 None 禁用缓存）
    let mtime = vault::mtime(&app, &entry_ref).await.unwrap_or(None);

    if let Some(mt) = mtime {
        let cache = render_cache();
        // 锁中毒时继续用内部值，命令路径不允许 panic（改进计划 B3）
        if let Some(hit) = cache.lock().unwrap_or_else(|e| e.into_inner()).get(&entry) {
            if hit.mtime == mt {
                return Ok(hit.result.clone());
            }
        }
    }

    let text = vault::read_text(&app, &entry_ref).await?;
    // 解析 + 消毒是纯 CPU 计算，放到阻塞线程池（改进计划 B2），
    // 否则大文档会把 tokio worker 占满，其它命令排队。
    let result = tauri::async_runtime::spawn_blocking(move || markdown::render_document(&text))
        .await
        .map_err(|e| AppError::InvalidInput(format!("渲染线程失败: {e}")))?;

    if let Some(mt) = mtime {
        let mut cache = render_cache().lock().unwrap_or_else(|e| e.into_inner());
        if cache.len() >= RENDER_CACHE_CAP && !cache.contains_key(&entry) {
            cache.clear();
        }
        cache.insert(entry, CacheEntry { mtime: mt, result: result.clone() });
    }

    Ok(result)
}

/// 解析文档内的相对引用（改进计划 A1）：图片与 md 互跳共用这一个入口。
/// 前端不再做任何路径运算，平台差异全部收敛在 Rust。
#[tauri::command]
async fn resolve_entry(app: AppHandle, doc_entry: String, rel: String) -> AppResult<String> {
    vault::resolve_entry(&app, &EntryRef(doc_entry), &rel)
        .await
        .map(|e| e.0)
}

/// 读取文档内相对路径指向的图片，返回 data URL（改进计划 A1）。
#[tauri::command]
async fn read_image_in_doc(app: AppHandle, doc_entry: String, rel: String) -> AppResult<String> {
    vault::read_image_in_doc(&app, &EntryRef(doc_entry), &rel).await
}

/// 读取临时分享的单文件 URI（方案 §7.4）。
/// 分享进来的 content:// URI 是临时授权，不属于任何 vault，
/// 图片相对路径无法 resolve，前端会收到 NOT_FOUND 并降级显示。
#[tauri::command]
async fn read_temp_doc(app: AppHandle, uri: String) -> AppResult<RenderResult> {
    let entry_ref = EntryRef(uri);
    let text = vault::read_text(&app, &entry_ref).await?;
    Ok(markdown::render_document(&text))
}

/// 只读源码视图：返回原始 Markdown 文本，不做任何渲染。
/// 用于「看一眼原文」——排错（公式/表格没渲染对）和复制原始语法。
#[tauri::command]
async fn read_raw_doc(app: AppHandle, entry: String) -> AppResult<String> {
    vault::read_text(&app, &EntryRef(entry)).await
}

/// 抽取标题与纯文本，供前端建 FTS5 索引（方案 §7.3）。
/// 只做一次 parser 遍历，不生成 HTML、不分块、不做消毒（改进计划 A5）。
#[derive(Debug, Clone, Serialize)]
pub struct PlainDoc {
    pub entry: String,
    pub title: Option<String>,
    pub plain: String,
    pub mtime: i64,
}

#[tauri::command]
async fn extract_plain(app: AppHandle, entry: String) -> AppResult<PlainDoc> {
    let entry_ref = EntryRef(entry);
    let mtime = vault::mtime(&app, &entry_ref).await.ok().flatten().unwrap_or(0);
    let text = vault::read_text(&app, &entry_ref).await?;
    let doc = tauri::async_runtime::spawn_blocking(move || markdown::extract_index(&text))
        .await
        .map_err(|e| AppError::InvalidInput(format!("索引线程失败: {e}")))?;
    Ok(PlainDoc {
        entry: entry_ref.0,
        title: doc.title,
        plain: doc.plain,
        mtime,
    })
}

/// 递归收集库内全部 Markdown 文件（方案 §8.5 建索引）。带 mtime 供增量判断。
#[tauri::command]
async fn collect_markdown_files(app: AppHandle, root: String) -> AppResult<Vec<FileNode>> {
    vault::collect_markdown(&app, &VaultRef(root)).await
}

fn default_measure() -> String {
    "comfortable".to_string()
}

fn default_rail_width() -> f64 {
    258.0
}

fn default_outline_width() -> f64 {
    288.0
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    pub theme: String,
    #[serde(rename = "fontSize", default)]
    pub font_size: Option<f64>,
    #[serde(rename = "lineHeight", default)]
    pub line_height: Option<f64>,
    /// 正文宽度档位：compact / comfortable / wide（旧配置文件缺省为舒适）
    #[serde(default = "default_measure")]
    pub measure: String,
    /// 左侧资料栏宽度（桌面端可拖拽，px）
    #[serde(rename = "railWidth", default = "default_rail_width")]
    pub rail_width: f64,
    /// 右侧大纲宽度（桌面端可拖拽，px）
    #[serde(rename = "outlineWidth", default = "default_outline_width")]
    pub outline_width: f64,
}

/// 偏好设置：持久化到 app 数据目录的 JSON（比走 SQL 更轻，且启动时无需等 DB）
#[tauri::command]
async fn save_prefs(app: AppHandle, prefs: Prefs) -> AppResult<()> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    std::fs::create_dir_all(&dir)?;
    let path = dir.join("prefs.json");
    let json = serde_json::to_string_pretty(&prefs)
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    std::fs::write(path, json)?;
    Ok(())
}

#[tauri::command]
async fn load_prefs(app: AppHandle) -> AppResult<Prefs> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|e| AppError::InvalidInput(e.to_string()))?;
    let path = dir.join("prefs.json");
    match std::fs::read_to_string(path) {
        Ok(json) => serde_json::from_str(&json).map_err(|e| AppError::InvalidInput(e.to_string())),
        Err(_) => Ok(Prefs {
            theme: "github-light".into(),
            font_size: None,
            line_height: None,
            measure: default_measure(),
            rail_width: default_rail_width(),
            outline_width: default_outline_width(),
        }),
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Android 下初始化 log → logcat，否则 log::info! 等全部静默
    #[cfg(target_os = "android")]
    android_logger::init_once(
        android_logger::Config::default()
            .with_max_level(log::LevelFilter::Info)
            .with_tag("lnkwell"),
    );

    // mut 仅在追加平台插件时需要，桌面端会报 unused_mut
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default();

    // 桌面单实例：须最先注册（官方要求）。已在运行时再次双击 md 文件，
    // 把文件路径作为 open-shared-file 事件转发给既有实例并聚焦窗口，
    // 前端 nav.listenForShare 已监听该事件。
    #[cfg(desktop)]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(
            |app, argv, _cwd| {
                if let Some(file) = find_shared_file(argv) {
                    let _ = app.emit("open-shared-file", file);
                }
                if let Some(win) = app.get_webview_window("main") {
                    let _ = win.unminimize();
                    let _ = win.set_focus();
                }
            },
        ));
    }

    builder = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init())
        // 外链交给系统浏览器（改进计划 A10）
        .plugin(tauri_plugin_opener::init());

    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(tauri_plugin_android_fs::init());
    }

    builder
        .setup(|_app| {
            // 桌面冷启动：双击/打开方式带来的 md 文件路径暂存，等前端来取
            #[cfg(desktop)]
            {
                let args = std::env::args().skip(1).collect::<Vec<_>>();
                if let Some(file) = find_shared_file(args) {
                    log::info!("startup shared file: {file}");
                    *pending_share().lock().unwrap_or_else(|e| e.into_inner()) = Some(file);
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            pick_vault,
            check_vault_permission,
            list_dir,
            read_doc,
            read_image,
            read_image_in_doc,
            resolve_entry,
            is_mobile,
            export_pdf,
            read_temp_doc,
            read_raw_doc,
            extract_plain,
            collect_markdown_files,
            save_prefs,
            load_prefs,
            take_pending_share,
        ])
        .run(tauri::generate_context!())
        .unwrap_or_else(|e| {
            // 启动失败只记录日志，不用 expect()（改进计划 B3）
            log::error!("tauri application failed to start: {e}");
            std::process::exit(1);
        });
}
