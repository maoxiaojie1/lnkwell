//! Android 实现：SAF content:// 访问（方案 §11）
//! EntryRef / VaultRef 均承载 FsUri 的 JSON 序列化串；文档与目录条目由 read_dir 返回完整 document URI，
//! 因此读取时直接用 entry 自身 URI，无需从根逐级解析（SAF 无文件系统路径）。

use super::{is_image, is_markdown, sort_nodes, EntryRef, FileNode, VaultRef};
use crate::error::{AppError, AppResult};
use crate::markdown::{decode_text, normalize_newlines, too_large, MAX_FILE_SIZE};
use tauri::AppHandle;
use tauri_plugin_android_fs::{AndroidFsExt, Entry, FsUri, UriPermission};

fn map_err(e: tauri_plugin_android_fs::Error) -> AppError {
    let msg = e.to_string().to_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("security") {
        AppError::PermissionExpired
    } else {
        AppError::InvalidInput(e.to_string())
    }
}

fn parse_uri(s: &str) -> AppResult<FsUri> {
    FsUri::from_json_str(s).map_err(map_err)
}

pub async fn pick_root(app: &AppHandle) -> AppResult<Option<VaultRef>> {
    let afs = app.android_fs_async();
    let picked = afs
        .picker()
        .pick_dir(None, true)
        .await
        .map_err(map_err)?;
    match picked {
        Some(uri) => {
            // 持久化 SAF 权限，重启后仍可用（方案 §11.4）
            afs.picker().persist_uri_permission(&uri).await.map_err(map_err)?;
            Ok(Some(VaultRef(uri.to_json_string().map_err(map_err)?)))
        }
        None => Ok(None),
    }
}

pub async fn list(app: &AppHandle, dir: &EntryRef) -> AppResult<Vec<FileNode>> {
    let afs = app.android_fs_async();
    let uri = parse_uri(&dir.0)?;
    let entries = afs.read_dir(&uri).await.map_err(map_err)?;
    log::info!("read_dir -> {} raw entries", entries.len());
    for e in &entries {
        match e {
            Entry::Dir { name, .. } => log::info!("  [dir] {name}"),
            Entry::File { name, mime_type, .. } => log::info!("  [file] {name} ({mime_type})"),
        }
    }
    let mut nodes: Vec<FileNode> = entries
        .into_iter()
        .filter_map(|e| match e {
            Entry::Dir { uri, name, .. } => {
                if name.starts_with('.') {
                    None
                } else {
                    uri.to_json_string().ok().map(|s| FileNode::dir(name, s))
                }
            }
            Entry::File { uri, name, .. } => {
                if name.starts_with('.') || !(is_markdown(&name) || is_image(&name)) {
                    None
                } else {
                    uri.to_json_string().ok().map(|s| FileNode::file(name, s))
                }
            }
        })
        .collect();
    sort_nodes(&mut nodes);
    log::info!("list -> {} nodes after filter", nodes.len());
    Ok(nodes)
}

pub async fn read_text(app: &AppHandle, f: &EntryRef) -> AppResult<String> {
    let afs = app.android_fs_async();
    let uri = parse_uri(&f.0)?;
    let len = afs.get_len(&uri).await.map_err(map_err)?;
    if len > MAX_FILE_SIZE {
        return Err(too_large(len));
    }
    let bytes = afs.read(&uri).await.map_err(map_err)?;
    let (text, _enc) = decode_text(&bytes)?;
    Ok(normalize_newlines(&text))
}

/// 读取图片原始字节（content:// URI 经 android-fs 读取，供上层编码 data URL）
pub async fn read_image_bytes(app: &AppHandle, f: &EntryRef) -> AppResult<Vec<u8>> {
    let afs = app.android_fs_async();
    let uri = parse_uri(&f.0)?;
    let len = afs.get_len(&uri).await.map_err(map_err)?;
    const MAX_IMAGE_SIZE: u64 = 32 * 1024 * 1024;
    if len > MAX_IMAGE_SIZE {
        return Err(too_large(len));
    }
    afs.read(&uri).await.map_err(map_err)
}

pub async fn collect_markdown(app: &AppHandle, root: &VaultRef) -> AppResult<Vec<FileNode>> {
    let afs = app.android_fs_async();
    let root_uri = parse_uri(&root.0)?;
    let mut out = Vec::new();
    let mut stack = vec![root_uri];
    while let Some(dir_uri) = stack.pop() {
        let entries = match afs.read_dir(&dir_uri).await {
            Ok(e) => e,
            Err(e) => {
                log::warn!("skip unreadable dir: {e}");
                continue;
            }
        };
        for e in entries {
            match e {
                Entry::Dir { uri, name, .. } => {
                    if !name.starts_with('.') {
                        stack.push(uri);
                    }
                }
                Entry::File { uri, name, .. } => {
                    if !name.starts_with('.') && is_markdown(&name) {
                        if let Ok(s) = uri.to_json_string() {
                            out.push(FileNode::file(name, s));
                        }
                    }
                }
            }
        }
    }
    sort_nodes(&mut out);
    Ok(out)
}

/// SAF 下相对图片需基于「文档所在目录的 tree」解析。resolve_img 是同步签名，
/// 无法逐级 resolve_file_uri；采用方案：entry JSON 内含 document URI，
/// 图片相对路径经 URL 规范的相对解析拼到父目录 document URI 上。
/// 对 SAF ExternalDocumentsProvider，document URI 形如
/// content://<auth>/tree/<tree>/document/<doc>，子项可直接以 docId 拼接。
pub fn resolve_img(doc_entry: &EntryRef, rel: &str) -> String {
    let uri = match parse_uri(&doc_entry.0) {
        Ok(u) => u,
        Err(_) => return rel.to_owned(),
    };
    let doc = uri.uri.to_string();
    // 取文档 URI 的父级：去掉最后一段 documentId 得到目录 document URI
    let parent = match doc.rfind('/') {
        Some(i) => &doc[..i],
        None => &doc,
    };
    // 规范化 rel：去 ./ 前缀，保留 ../ 由 provider 侧处理
    let rel_clean = rel.trim_start_matches("./");
    // 统一 md-file:// 哨兵（与桌面一致），前端 readImage 时会再经 fs.read_file 读取
    format!("md-file://{parent}/{rel_clean}")
}

pub async fn check_permission(app: &AppHandle, vault: &VaultRef) -> AppResult<bool> {
    let afs = app.android_fs_async();
    let uri = parse_uri(&vault.0)?;
    afs.picker()
        .check_persisted_uri_permission(&uri, UriPermission::Read)
        .await
        .map_err(map_err)
}
