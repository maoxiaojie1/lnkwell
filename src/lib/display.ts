/**
 * 条目引用的展示工具（改进计划 C2）。
 *
 * `entry` 是**不透明句柄**：桌面端是绝对路径，Android 端是 FsUri 的 JSON 串，
 * 形如 `{"uri":"content://.../document/primary%3ADocs%2Freadme.md","documentTopTreeUri":...}`。
 * UI 不能直接把它当路径显示 —— 否则 Android 上会给用户看一整段 JSON。
 */

/** 还原出可读 URI 字符串（Android 的 FsUri JSON → content:// 串，并做一次 percent-decode） */
function unwrapUri(entry: string): string {
  let s = entry;
  if (s.startsWith('{')) {
    try {
      const o = JSON.parse(s) as { uri?: unknown };
      if (typeof o.uri === 'string') s = o.uri;
    } catch {
      /* 非法 JSON：按原样处理 */
    }
  }
  try {
    s = decodeURIComponent(s);
  } catch {
    /* 非法转义：保留原样 */
  }
  return s;
}

/** 取条目的人类可读文件名（不含目录） */
export function entryName(entry: string): string {
  const s = unwrapUri(entry);
  const seg = s.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? s;
  // SAF 的 docId 形如 `primary:Documents/mdvault/x.md`，末段偶尔还带冒号
  return seg.includes(':') ? (seg.split(':').pop() ?? seg) : seg;
}

/** 展示用路径：优先去掉库根前缀；无法还原时退回文件名 */
export function displayPath(entry: string, root?: string | null): string {
  if (root && entry.startsWith(root)) {
    const rel = entry.slice(root.length).replace(/^[\\/]+/, '');
    if (rel) return rel;
  }
  // Android：content:// 前缀对用户无意义，只展示文件名
  if (unwrapUri(entry) !== entry) return entryName(entry);
  return entry;
}
