//! 文件访问抽象（方案 §4.4）。
//! 不用 trait —— 含 async fn 的 trait 非 dyn 兼容；用 #[cfg] 编译期分发，零运行时开销。

#[cfg(target_os = "android")]
pub mod android;
#[cfg(not(target_os = "android"))]
pub mod desktop;

#[cfg(target_os = "android")]
use android as imp;
#[cfg(not(target_os = "android"))]
use desktop as imp;

use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use tauri::AppHandle;

/// 库标识：Android 上是 SAF tree URI，桌面上是绝对路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRef(pub String);

/// 库内条目标识
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRef(pub String);

#[derive(Debug, Clone, Serialize)]
pub struct FileNode {
    pub name: String,
    /// 条目引用：桌面为绝对路径，Android 为 document URI
    pub entry: String,
    pub is_dir: bool,
}

impl FileNode {
    pub fn file(name: String, entry: String) -> Self {
        Self { name, entry, is_dir: false }
    }
    pub fn dir(name: String, entry: String) -> Self {
        Self { name, entry, is_dir: true }
    }
}

/// 扩展名大小写不敏感，覆盖 .md / .markdown / .mdx（方案 §4.2）
pub fn is_markdown(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdx")
}

/// 文档内可引用的图片类型（文件树展示与 ImageViewer 打开用）
pub fn is_image(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".gif")
        || lower.ends_with(".webp")
        || lower.ends_with(".bmp")
        || lower.ends_with(".svg")
        || lower.ends_with(".ico")
}

/// 读取图片并编码为 data URL（文件树图片预览 / md 内嵌图片加载共用）。
/// 返回 `data:<mime>;base64,...`，前端直接赋给 img.src，
/// 绕开 asset 协议与 IPC 字节传输的不确定性（方案 §4.3 修复记录）。
pub async fn read_image(app: &AppHandle, f: &EntryRef) -> AppResult<String> {
    let bytes = imp::read_image_bytes(app, f).await?;
    let mime = image_mime_by_ext(f);
    use base64::Engine as _;
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    Ok(format!("data:{mime};base64,{b64}"))
}

/// 由文件扩展名推断 MIME（仅 Rust 端图片预览使用）
fn image_mime_by_ext(f: &EntryRef) -> &'static str {
    let lower = f.0.to_ascii_lowercase();
    if lower.ends_with(".png") { "image/png" }
    else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") { "image/jpeg" }
    else if lower.ends_with(".gif") { "image/gif" }
    else if lower.ends_with(".webp") { "image/webp" }
    else if lower.ends_with(".bmp") { "image/bmp" }
    else if lower.ends_with(".svg") { "image/svg+xml" }
    else if lower.ends_with(".ico") { "image/x-icon" }
    else { "application/octet-stream" }
}

pub async fn pick_root(app: &AppHandle) -> AppResult<Option<VaultRef>> {
    imp::pick_root(app).await
}

pub async fn list(app: &AppHandle, dir: &EntryRef) -> AppResult<Vec<FileNode>> {
    imp::list(app, dir).await
}

pub async fn read_text(app: &AppHandle, f: &EntryRef) -> AppResult<String> {
    imp::read_text(app, f).await
}

/// 递归收集库内全部 Markdown 文件（建索引用）
pub async fn collect_markdown(app: &AppHandle, root: &VaultRef) -> AppResult<Vec<FileNode>> {
    imp::collect_markdown(app, root).await
}

/// 把文档内的相对图片路径解析为可注入 src 的字符串
pub fn resolve_img(doc_entry: &EntryRef, rel: &str) -> String {
    imp::resolve_img(doc_entry, rel)
}

/// 校验 SAF 权限是否仍有效（桌面端恒 true，见方案 §11.4）
pub async fn check_permission(app: &AppHandle, vault: &VaultRef) -> AppResult<bool> {
    imp::check_permission(app, vault).await
}

/// 目录在前、同类按名称自然排序（10.md 排在 9.md 之后，方案 §4.2）
pub fn sort_nodes(nodes: &mut [FileNode]) {
    nodes.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| natural_cmp(&a.name, &b.name))
    });
}

fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let mut ca = a.chars().peekable();
    let mut cb = b.chars().peekable();
    loop {
        match (ca.peek(), cb.peek()) {
            (None, None) => return std::cmp::Ordering::Equal,
            (None, Some(_)) => return std::cmp::Ordering::Less,
            (Some(_), None) => return std::cmp::Ordering::Greater,
            (Some(x), Some(y)) if x.is_ascii_digit() && y.is_ascii_digit() => {
                let na = take_number(&mut ca);
                let nb = take_number(&mut cb);
                match na.cmp(&nb) {
                    std::cmp::Ordering::Equal => continue,
                    ord => return ord,
                }
            }
            (Some(x), Some(y)) => {
                let ord = x.to_lowercase().cmp(y.to_lowercase());
                if ord != std::cmp::Ordering::Equal {
                    return ord;
                }
                ca.next();
                cb.next();
            }
        }
    }
}

fn take_number(it: &mut std::iter::Peekable<std::str::Chars>) -> u64 {
    let mut n: u64 = 0;
    while let Some(c) = it.peek() {
        if let Some(d) = c.to_digit(10) {
            n = n.saturating_mul(10).saturating_add(u64::from(d));
            it.next();
        } else {
            break;
        }
    }
    n
}
