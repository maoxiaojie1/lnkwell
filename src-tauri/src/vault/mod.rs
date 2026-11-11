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
    /// 文件修改时间（Unix 毫秒）。桌面端有值；Android SAF 无稳定 mtime 时为 None。
    /// 供增量索引判断（改进计划 A4）。
    #[serde(default)]
    pub mtime: Option<i64>,
}

impl FileNode {
    pub fn file(name: String, entry: String) -> Self {
        Self {
            name,
            entry,
            is_dir: false,
            mtime: None,
        }
    }
    pub fn dir(name: String, entry: String) -> Self {
        Self {
            name,
            entry,
            is_dir: true,
            mtime: None,
        }
    }
    pub fn with_mtime(mut self, mtime: Option<i64>) -> Self {
        self.mtime = mtime;
        self
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
    Ok(encode_data_url(f, &bytes))
}

/// 读取「文档内相对路径」指向的图片（改进计划 A1）。
/// 平台差异全部收敛到 `resolve_entry`，此处只做组合，不再让前端拼路径。
pub async fn read_image_in_doc(app: &AppHandle, doc: &EntryRef, rel: &str) -> AppResult<String> {
    let entry = resolve_entry(app, doc, rel).await?;
    let bytes = imp::read_image_bytes(app, &entry).await?;
    Ok(encode_data_url(&entry, &bytes))
}

/// base64 编码为 data URL，前端可直接赋给 img.src
///
/// MIME **先按字节魔数判断**，扩展名只作兜底：Android 的 `EntryRef` 是
/// `{"uri":"content://…"}` 这样的 JSON 串，本身不带扩展名，只看扩展名会一律得到
/// `application/octet-stream`。而 SVG 不像 PNG 能靠嗅探补救——Chrome 对非图片 MIME
/// 的 data URL 直接拒绝渲染，表现为图片位置空白。
fn encode_data_url(f: &EntryRef, bytes: &[u8]) -> String {
    use base64::Engine as _;
    let mime = image_mime(f, bytes);
    let b64 = base64::engine::general_purpose::STANDARD.encode(bytes);
    format!("data:{mime};base64,{b64}")
}

fn image_mime(f: &EntryRef, bytes: &[u8]) -> &'static str {
    sniff_image_mime(bytes).unwrap_or_else(|| image_mime_by_ext(f))
}

/// 按文件魔数识别图片格式（跨平台可靠，不依赖路径形态）
fn sniff_image_mime(b: &[u8]) -> Option<&'static str> {
    if b.starts_with(&[0x89, 0x50, 0x4E, 0x47]) {
        return Some("image/png");
    }
    if b.starts_with(&[0xFF, 0xD8, 0xFF]) {
        return Some("image/jpeg");
    }
    if b.starts_with(b"GIF87a") || b.starts_with(b"GIF89a") {
        return Some("image/gif");
    }
    if b.len() > 12 && b.starts_with(b"RIFF") && &b[8..12] == b"WEBP" {
        return Some("image/webp");
    }
    if b.starts_with(b"BM") {
        return Some("image/bmp");
    }
    if b.starts_with(&[0x00, 0x00, 0x01, 0x00]) {
        return Some("image/x-icon");
    }
    // SVG 是文本：BOM / 注释 / XML 声明都可能出现在根元素之前，扫前 1KB 即可
    let head = &b[..b.len().min(1024)];
    if head.windows(4).any(|w| w.eq_ignore_ascii_case(b"<svg")) {
        return Some("image/svg+xml");
    }
    None
}

/// 由文件扩展名推断 MIME（仅作魔数识别失败时的兜底；桌面端路径带扩展名）
fn image_mime_by_ext(f: &EntryRef) -> &'static str {
    let lower = f.0.to_ascii_lowercase();
    if lower.ends_with(".png") {
        "image/png"
    } else if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
        "image/jpeg"
    } else if lower.ends_with(".gif") {
        "image/gif"
    } else if lower.ends_with(".webp") {
        "image/webp"
    } else if lower.ends_with(".bmp") {
        "image/bmp"
    } else if lower.ends_with(".svg") {
        "image/svg+xml"
    } else if lower.ends_with(".ico") {
        "image/x-icon"
    } else {
        "application/octet-stream"
    }
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

/// 文件修改时间（Unix 毫秒）。用于渲染缓存失效判断。
/// 桌面端返回真实 mtime；Android SAF 不提供稳定 mtime，返回 None（缓存禁用）。
pub async fn mtime(app: &AppHandle, f: &EntryRef) -> AppResult<Option<i64>> {
    imp::mtime(app, f).await
}

/// 递归收集库内全部 Markdown 文件（建索引用）
pub async fn collect_markdown(app: &AppHandle, root: &VaultRef) -> AppResult<Vec<FileNode>> {
    imp::collect_markdown(app, root).await
}

/// 把文档内的相对引用解析为完整条目引用（改进计划 A1）。
/// 这是**唯一**的相对路径解析入口，前端不再做任何路径运算。
pub async fn resolve_entry(app: &AppHandle, doc: &EntryRef, rel: &str) -> AppResult<EntryRef> {
    imp::resolve_entry(app, doc, rel).await
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sniffs_svg_regardless_of_entry_shape() {
        // Android 的 EntryRef 是 content:// 的 JSON 串，没有扩展名
        let android_entry =
            EntryRef(r#"{"uri":"content://x/document/primary%3ADownload%2Fa"}"#.into());
        let svg = br#"<?xml version="1.0"?><svg xmlns="http://www.w3.org/2000/svg"></svg>"#;
        assert_eq!(image_mime(&android_entry, svg), "image/svg+xml");
        assert!(encode_data_url(&android_entry, svg).starts_with("data:image/svg+xml;base64,"));
    }

    #[test]
    fn sniffs_svg_after_bom_and_leading_text() {
        let mut bytes = vec![0xEF, 0xBB, 0xBF];
        bytes.extend_from_slice(b"\n<!-- c -->\n<SVG viewBox=\"0 0 1 1\"></SVG>");
        assert_eq!(sniff_image_mime(&bytes), Some("image/svg+xml"));
    }

    #[test]
    fn sniffs_raster_formats() {
        assert_eq!(sniff_image_mime(b"\x89PNG\r\n\x1a\n"), Some("image/png"));
        assert_eq!(sniff_image_mime(b"\xFF\xD8\xFF\xE0"), Some("image/jpeg"));
        assert_eq!(sniff_image_mime(b"GIF89a...."), Some("image/gif"));
        assert_eq!(
            sniff_image_mime(b"RIFF\0\0\0\0WEBPVP8 "),
            Some("image/webp")
        );
        assert_eq!(sniff_image_mime(b"BM\0\0"), Some("image/bmp"));
    }

    #[test]
    fn falls_back_to_extension_for_unknown_bytes() {
        assert_eq!(sniff_image_mime(b"not-an-image"), None);
        assert_eq!(
            image_mime(&EntryRef("D:\\img\\a.PNG".into()), b"not-an-image"),
            "image/png"
        );
        // 两边都判断不出来时保持原来的兜底值
        assert_eq!(
            image_mime(&EntryRef("weird".into()), b"???"),
            "application/octet-stream"
        );
    }
}
