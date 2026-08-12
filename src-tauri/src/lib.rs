mod error;
pub mod markdown;
mod vault;

use error::{AppError, AppResult};
use markdown::RenderResult;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use vault::{EntryRef, FileNode, VaultRef};

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

/// 读取并渲染文档（方案 §5.1 数据流）
#[tauri::command]
async fn read_doc(app: AppHandle, entry: String) -> AppResult<RenderResult> {
    let entry_ref = EntryRef(entry);
    let text = vault::read_text(&app, &entry_ref).await?;
    let resolver_entry = entry_ref.clone();
    markdown::render_document(&text, &move |rel| vault::resolve_img(&resolver_entry, rel))
}

/// 读取临时分享的单文件 URI（方案 §7.4）。
/// 分享进来的 content:// URI 是临时授权，不属于任何 vault，
/// 图片相对路径无法 resolve，直接返回空（纯文本 Markdown 不影响）。
#[tauri::command]
async fn read_temp_doc(app: AppHandle, uri: String) -> AppResult<RenderResult> {
    let entry_ref = EntryRef(uri);
    let text = vault::read_text(&app, &entry_ref).await?;
    markdown::render_document(&text, &|_rel| String::new())
}

/// 提取纯文本与标题，供前端建 FTS5 索引（方案 §7.3）
/// Clone 是 app.emit 的要求
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
    let text = vault::read_text(&app, &entry_ref).await?;
    let result = markdown::render_document(&text, &|rel| rel.to_owned())?;
    Ok(PlainDoc {
        entry: entry_ref.0,
        title: result.title,
        plain: result.plain,
        mtime: 0,
    })
}

/// 递归收集库内全部 Markdown 文件（方案 §8.5 建索引）
#[tauri::command]
async fn collect_markdown_files(app: AppHandle, root: String) -> AppResult<Vec<FileNode>> {
    vault::collect_markdown(&app, &VaultRef(root)).await
}

/// 后台建索引：逐个抽取纯文本并 emit 进度事件（方案 §8.5）。
/// 单篇失败不中断整批。
#[tauri::command]
async fn build_index(app: AppHandle, root: String) -> AppResult<u32> {
    let files = vault::collect_markdown(&app, &VaultRef(root)).await?;
    let total = files.len();
    let mut indexed: u32 = 0;
    for (i, f) in files.iter().enumerate() {
        match vault::read_text(&app, &EntryRef(f.entry.clone())).await {
            Ok(text) => {
                match markdown::render_document(&text, &|rel| rel.to_owned()) {
                    Ok(r) => {
                        app.emit(
                            "index-file",
                            PlainDoc {
                                entry: f.entry.clone(),
                                title: r.title,
                                plain: r.plain,
                                mtime: 0,
                            },
                        )
                        .ok();
                        indexed += 1;
                    }
                    Err(e) => log::warn!("render failed for {}: {e}", f.entry),
                }
            }
            Err(e) => log::warn!("index failed for {}: {e}", f.entry),
        }
        if i % 10 == 0 {
            app.emit("index-progress", (i, total)).ok();
        }
    }
    app.emit("index-done", total).ok();
    Ok(indexed)
}

fn default_measure() -> String {
    "comfortable".to_string()
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct Prefs {
    pub theme: String,
    #[serde(rename = "fontSize")]
    pub font_size: f64,
    #[serde(rename = "lineHeight")]
    pub line_height: f64,
    /// 正文宽度档位：compact / comfortable / wide（旧配置文件缺省为舒适）
    #[serde(default = "default_measure")]
    pub measure: String,
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
            font_size: 16.0,
            line_height: 1.7,
            measure: default_measure(),
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

    // mut 仅在 Android 下追加插件时需要，桌面端会报 unused_mut
    #[allow(unused_mut)]
    let mut builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_sql::Builder::default().build())
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_deep_link::init());

    #[cfg(target_os = "android")]
    {
        builder = builder.plugin(tauri_plugin_android_fs::init());
    }

    builder
        .invoke_handler(tauri::generate_handler![
            pick_vault,
            check_vault_permission,
            list_dir,
            read_doc,
            read_image,
            read_temp_doc,
            extract_plain,
            collect_markdown_files,
            build_index,
            save_prefs,
            load_prefs,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
