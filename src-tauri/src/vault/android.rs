//! Android 实现：SAF content:// 访问（方案 §11）
//! EntryRef / VaultRef 均承载 FsUri 的 JSON 序列化串；文档与目录条目由 read_dir 返回完整 document URI，
//! 因此读取时直接用 entry 自身 URI，无需从根逐级解析（SAF 无文件系统路径）。

use super::{is_image, is_markdown, sort_nodes, EntryRef, FileNode, VaultRef};
use crate::error::{AppError, AppResult};
use crate::markdown::{decode_text, normalize_newlines, too_large, MAX_FILE_SIZE};
use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;
use tauri_plugin_android_fs::{AndroidFsExt, Entry, FsUri, UriPermission};

/// 父目录映射（改进计划 A1）：子条目 URI（原始 content:// 串）→ 父目录 FsUri JSON。
///
/// 为什么需要它：SAF 没有「`../`」的概念，相对引用必须基于一个**已知目录 URI**
/// 才能交给 `resolve_file_uri` 解析；而文档 entry 只携带自己的 document URI。
/// 因此在每次 `read_dir` 时记住父子关系，`resolve_entry` 时即可反查。
/// 进程内缓存，随库遍历（目录树展开 / 索引）逐步补全。
static PARENT_DIRS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();

fn parent_dirs() -> &'static Mutex<HashMap<String, String>> {
    PARENT_DIRS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn remember_parent(dir: &FsUri, child: &FsUri) {
    if let Ok(json) = dir.to_json_string() {
        parent_dirs()
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(child.uri.clone(), json);
    }
}

fn lookup_parent(child_uri: &str) -> Option<String> {
    parent_dirs()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .get(child_uri)
        .cloned()
}

fn map_err(e: tauri_plugin_android_fs::Error) -> AppError {
    let msg = e.to_string().to_lowercase();
    if msg.contains("permission") || msg.contains("denied") || msg.contains("security") {
        AppError::PermissionExpired
    } else if msg.contains("not found") || msg.contains("no such") {
        AppError::NotFound
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
    let dir_uri = parse_uri(&dir.0)?;
    let entries = afs.read_dir(&dir_uri).await.map_err(map_err)?;
    // 记录父子关系，供文档内相对图片/互跳解析（改进计划 A1）
    for e in &entries {
        match e {
            Entry::Dir { uri, .. } | Entry::File { uri, .. } => remember_parent(&dir_uri, uri),
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

/// SAF 不提供稳定的文件 mtime，无法可靠判断缓存失效，返回 None（渲染缓存禁用）。
pub async fn mtime(_app: &AppHandle, _f: &EntryRef) -> AppResult<Option<i64>> {
    Ok(None)
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
                        remember_parent(&dir_uri, &uri);
                        stack.push(uri);
                    }
                }
                Entry::File { uri, name, .. } => {
                    if !name.starts_with('.') && is_markdown(&name) {
                        remember_parent(&dir_uri, &uri);
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

/// 相对引用解析（改进计划 A1）。
///
/// 步骤：
/// 1. 由「子 URI → 父目录 URI」映射反查文档所在目录；
/// 2. 前导 `../` 逐级上溯（同样靠映射表，根目录处失败）；
/// 3. 剩余路径交给 `AndroidFs::resolve_file_uri`（要求目标真实存在）。
pub async fn resolve_entry(app: &AppHandle, doc: &EntryRef, rel: &str) -> AppResult<EntryRef> {
    let afs = app.android_fs_async();
    let doc_uri = parse_uri(&doc.0)?;

    // 1) 文档所在目录
    let mut dir = match lookup_parent(&doc_uri.uri) {
        Some(json) => FsUri::from_json_str(&json).map_err(map_err)?,
        None => {
            log::warn!("resolve_entry: 父目录未知（请先遍历目录）: {}", doc_uri.uri);
            return Err(AppError::NotFound);
        }
    };

    // 2) 拆解相对路径：前导 .. 上溯，其余保留交给 provider
    let normalized = rel.replace('\\', "/");
    let mut segments: Vec<&str> = Vec::new();
    let mut ups = 0usize;
    for seg in normalized.split('/') {
        match seg {
            "" | "." => {}
            ".." if segments.is_empty() => ups += 1,
            _ => segments.push(seg),
        }
    }
    for _ in 0..ups {
        let parent_json = lookup_parent(&dir.uri).ok_or(AppError::NotFound)?;
        dir = FsUri::from_json_str(&parent_json).map_err(map_err)?;
    }
    if segments.is_empty() {
        return Err(AppError::NotFound);
    }

    // 3) 交给 SAF 解析
    let resolved = afs
        .resolve_file_uri(&dir, segments.join("/"))
        .await
        .map_err(map_err)?;
    Ok(EntryRef(resolved.to_json_string().map_err(map_err)?))
}

pub async fn check_permission(app: &AppHandle, vault: &VaultRef) -> AppResult<bool> {
    let afs = app.android_fs_async();
    let uri = parse_uri(&vault.0)?;
    afs.picker()
        .check_persisted_uri_permission(&uri, UriPermission::Read)
        .await
        .map_err(map_err)
}
