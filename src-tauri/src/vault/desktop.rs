//! 桌面端实现：EntryRef = 绝对路径字符串。

use super::{is_image, is_markdown, EntryRef, FileNode, VaultRef};
use crate::error::{AppError, AppResult};
use crate::markdown::{decode_text, normalize_newlines, too_large, MAX_FILE_SIZE};
use std::path::{Path, PathBuf};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

pub async fn pick_root(app: &AppHandle) -> AppResult<Option<VaultRef>> {
    let picked = app.dialog().file().blocking_pick_folder();
    Ok(picked.map(|p| VaultRef(p.to_string())))
}

pub async fn list(_app: &AppHandle, dir: &EntryRef) -> AppResult<Vec<FileNode>> {
    let path = Path::new(&dir.0);
    let read = std::fs::read_dir(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound,
        std::io::ErrorKind::PermissionDenied => AppError::PermissionExpired,
        _ => AppError::Io(e),
    })?;

    let mut nodes = Vec::new();
    for entry in read {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        // 跳过隐藏目录/文件（.git 里可能有上万文件，见方案 §4.2）
        if name.starts_with('.') {
            continue;
        }
        let ft = entry.file_type()?;
        let full = entry.path().to_string_lossy().into_owned();
        if ft.is_dir() {
            nodes.push(FileNode::dir(name, full));
        } else if ft.is_file() && (is_markdown(&name) || is_image(&name)) {
            nodes.push(FileNode::file(name, full));
        }
    }
    sort_nodes(&mut nodes);
    Ok(nodes)
}

/// 读取图片原始字节（供上层编码 data URL）
pub async fn read_image_bytes(_app: &AppHandle, f: &EntryRef) -> AppResult<Vec<u8>> {
    let path = PathBuf::from(&f.0);
    // 文件 IO 一律扔到阻塞线程池（改进计划 B2），避免占住 tokio worker
    tauri::async_runtime::spawn_blocking(move || read_image_blocking(&path))
        .await
        .map_err(|e| AppError::InvalidInput(format!("读取线程失败: {e}")))?
}

fn read_image_blocking(path: &Path) -> AppResult<Vec<u8>> {
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound,
        _ => AppError::Io(e),
    })?;
    // 图片单独放宽上限到 32MB（超出一律拒绝防 OOM）
    const MAX_IMAGE_SIZE: u64 = 32 * 1024 * 1024;
    if meta.len() > MAX_IMAGE_SIZE {
        return Err(too_large(meta.len()));
    }
    Ok(std::fs::read(path)?)
}

pub async fn read_text(_app: &AppHandle, f: &EntryRef) -> AppResult<String> {
    let path = PathBuf::from(&f.0);
    tauri::async_runtime::spawn_blocking(move || read_text_blocking(&path))
        .await
        .map_err(|e| AppError::InvalidInput(format!("读取线程失败: {e}")))?
}

fn read_text_blocking(path: &Path) -> AppResult<String> {
    let meta = std::fs::metadata(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => AppError::NotFound,
        _ => AppError::Io(e),
    })?;
    if meta.len() > MAX_FILE_SIZE {
        return Err(too_large(meta.len()));
    }
    let bytes = std::fs::read(path)?;
    let (text, _enc) = decode_text(&bytes)?;
    Ok(normalize_newlines(&text))
}

/// 文件修改时间（Unix 毫秒）。文件不存在等异常返回 None（缓存退化为不命中）。
pub async fn mtime(_app: &AppHandle, f: &EntryRef) -> AppResult<Option<i64>> {
    let path = Path::new(&f.0);
    let meta = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return Ok(None),
    };
    let modified = match meta.modified() {
        Ok(t) => t,
        Err(_) => return Ok(None),
    };
    let ms = modified
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0);
    Ok(Some(ms))
}

pub async fn collect_markdown(_app: &AppHandle, root: &VaultRef) -> AppResult<Vec<FileNode>> {
    let mut out = Vec::new();
    let mut stack = vec![PathBuf::from(&root.0)];
    while let Some(dir) = stack.pop() {
        let read = match std::fs::read_dir(&dir) {
            Ok(r) => r,
            Err(e) => {
                log::warn!("skip unreadable dir {:?}: {e}", dir);
                continue;
            }
        };
        for entry in read.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') {
                continue;
            }
            let path = entry.path();
            match entry.file_type() {
                Ok(ft) if ft.is_dir() => stack.push(path),
                Ok(ft) if ft.is_file() && is_markdown(&name) => {
                    let mtime = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_millis() as i64);
                    out.push(
                        FileNode::file(name, path.to_string_lossy().into_owned()).with_mtime(mtime),
                    );
                }
                _ => {}
            }
        }
    }
    sort_nodes(&mut out);
    Ok(out)
}

pub fn resolve_entry_inner(doc: &str, rel: &str) -> String {
    let doc_path = Path::new(doc);
    let joined = if Path::new(rel).is_absolute() {
        // 绝对路径（含 Windows 盘符 / UNC）直接规范化
        PathBuf::from(rel)
    } else {
        let base = doc_path.parent().unwrap_or_else(|| Path::new(""));
        // 支持 ./ 前缀与 ../ 上溯（方案 §9.6 边界情况），由 normalize_path 收敛
        base.join(rel)
    };
    normalize_path(&joined).to_string_lossy().into_owned()
}

/// 相对路径解析（改进计划 A1 的唯一入口，桌面端部分）。
pub async fn resolve_entry(_app: &AppHandle, doc: &EntryRef, rel: &str) -> AppResult<EntryRef> {
    Ok(EntryRef(resolve_entry_inner(&doc.0, rel)))
}

pub async fn check_permission(_app: &AppHandle, vault: &VaultRef) -> AppResult<bool> {
    Ok(Path::new(&vault.0).is_dir())
}

/// 不依赖 canonicalize 的路径规范化（目标可能不存在时 canonicalize 会失败）
fn normalize_path(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            std::path::Component::ParentDir => {
                out.pop();
            }
            std::path::Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

use super::sort_nodes;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn natural_sort_orders_numerically() {
        let mut nodes = vec![
            FileNode::file("10.md".into(), "".into()),
            FileNode::file("9.md".into(), "".into()),
            FileNode::file("a.md".into(), "".into()),
            FileNode::dir("sub".into(), "".into()),
        ];
        sort_nodes(&mut nodes);
        let names: Vec<&str> = nodes.iter().map(|n| n.name.as_str()).collect();
        assert_eq!(names, vec!["sub", "9.md", "10.md", "a.md"]);
    }

    #[test]
    fn resolves_relative_image_against_doc_dir() {
        let doc = "D:\\docs\\note\\readme.md";
        assert_eq!(resolve_entry_inner(doc, "./img/a.png"), "D:\\docs\\note\\img\\a.png");
        assert_eq!(resolve_entry_inner(doc, "../assets/b.png"), "D:\\docs\\assets\\b.png");
        assert_eq!(
            resolve_entry_inner(doc, "sub/../img/c.png"),
            "D:\\docs\\note\\img\\c.png"
        );
    }

    #[test]
    fn keeps_absolute_path() {
        let doc = "D:\\docs\\note\\readme.md";
        assert_eq!(resolve_entry_inner(doc, "D:\\other\\x.md"), "D:\\other\\x.md");
    }

    #[test]
    fn markdown_extension_case_insensitive() {
        assert!(is_markdown("README.MD"));
        assert!(is_markdown("a.markdown"));
        assert!(is_markdown("b.mdx"));
        assert!(!is_markdown("c.txt"));
    }
}
