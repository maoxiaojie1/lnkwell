import { invoke } from '@tauri-apps/api/core';

/** 后端 AppError 序列化形态（方案 §9.5） */
export interface AppError {
  code:
    | 'PERMISSION_EXPIRED'
    | 'NOT_FOUND'
    | 'TOO_LARGE'
    | 'DECODE_FAILED'
    | 'IO'
    | 'DB'
    | 'INVALID_INPUT';
  message: string;
}

export function isAppError(e: unknown): e is AppError {
  return typeof e === 'object' && e !== null && 'code' in e && 'message' in e;
}

/** 按错误码给中文提示（方案 §9.5），返回 null 表示不该打扰用户 */
export function errorMessage(e: unknown): string {
  if (isAppError(e)) {
    switch (e.code) {
      case 'PERMISSION_EXPIRED':
        return '文档库访问权限已失效，请重新选择文件夹';
      case 'NOT_FOUND':
        return '文件不存在或已被移动';
      case 'TOO_LARGE':
        return '文件超过 10MB 上限，无法打开';
      case 'DECODE_FAILED':
        return '文件编码无法识别';
      default:
        return `出错了：${e.message}`;
    }
  }
  return e instanceof Error ? `出错了：${e.message}` : '未知错误';
}

export interface TocItem {
  level: number;
  text: string;
  id: string;
  /** 标题所在的分块序号，TOC 跳转时一步渲染到该块 */
  chunk: number;
}

export interface Frontmatter {
  title: string | null;
  date: string | null;
  tags: string[];
}

export interface RenderResult {
  chunks: string[];
  toc: TocItem[];
  title: string | null;
  /** YAML front matter 元数据（方案 §7.2），无则为 null */
  frontmatter: Frontmatter | null;
}

export interface FileNode {
  name: string;
  entry: string;
  is_dir: boolean;
  /** 文件修改时间（Unix 毫秒）；Android SAF 无 mtime 时为 undefined */
  mtime?: number;
}

export interface PlainDoc {
  entry: string;
  title: string | null;
  plain: string;
  mtime: number;
}

export interface PrefsDto {
  theme: string;
  /** 正文字号覆盖值；null 表示跟随主题默认 */
  fontSize: number | null;
  /** 行高覆盖值；null 表示跟随主题默认 */
  lineHeight: number | null;
  /** 正文宽度档位（旧版本配置可能缺省） */
  measure?: string;
  /** 左侧资料栏宽度（px，桌面端可拖拽） */
  railWidth?: number;
  /** 右侧大纲宽度（px，桌面端可拖拽） */
  outlineWidth?: number;
  /** 正文缩放倍数（1 = 100%，Ctrl + 滚轮 / Ctrl ±） */
  zoom?: number;
}

export const api = {
  pickVault: () => invoke<string | null>('pick_vault'),
  checkVaultPermission: (root: string) =>
    invoke<boolean>('check_vault_permission', { root }),
  listDir: (dir: string) => invoke<FileNode[]>('list_dir', { dir }),
  readDoc: (entry: string) => invoke<RenderResult>('read_doc', { entry }),
  /** 读取图片并返回 data URL（Rust 端 base64 编码），前端直接赋给 img.src */
  readImage: (file: string) => invoke<string>('read_image', { file }),
  /** 读取文档内相对路径指向的图片（改进计划 A1：路径解析只在 Rust 做） */
  readImageInDoc: (docEntry: string, rel: string) =>
    invoke<string>('read_image_in_doc', { docEntry, rel }),
  /** 解析文档内相对引用（md 互跳），返回完整条目引用 */
  resolveEntry: (docEntry: string, rel: string) =>
    invoke<string>('resolve_entry', { docEntry, rel }),
  /** 是否移动端（Android/iOS），用于隐藏桌面专属功能 */
  isMobile: () => invoke<boolean>('is_mobile'),
  /** 导出当前文档为 PDF（Windows 走 WebView2 PrintToPdf）。返回保存路径，取消返回 null */
  exportPdf: (suggestedName?: string) =>
    invoke<string | null>('export_pdf', { suggestedName: suggestedName ?? null }),
  readTempDoc: (uri: string) => invoke<RenderResult>('read_temp_doc', { uri }),
  /** 只读源码视图：返回原始 Markdown 文本（不做渲染） */
  readRawDoc: (entry: string) => invoke<string>('read_raw_doc', { entry }),
  /** 桌面端冷启动时双击/打开方式传入的待打开文件（取走即清），无则 null */
  takePendingShare: () => invoke<string | null>('take_pending_share'),
  extractPlain: (entry: string) => invoke<PlainDoc>('extract_plain', { entry }),
  collectMarkdownFiles: (root: string) =>
    invoke<FileNode[]>('collect_markdown_files', { root }),
  savePrefs: (prefs: PrefsDto) => invoke('save_prefs', { prefs }),
  loadPrefs: () => invoke<PrefsDto>('load_prefs'),
};
