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
  plain: string;
  /** YAML front matter 元数据（方案 §7.2），无则为 null */
  frontmatter: Frontmatter | null;
}

export interface FileNode {
  name: string;
  entry: string;
  is_dir: boolean;
}

export interface PlainDoc {
  entry: string;
  title: string | null;
  plain: string;
  mtime: number;
}

export interface PrefsDto {
  theme: string;
  fontSize: number;
  lineHeight: number;
  /** 正文宽度档位（旧版本配置可能缺省） */
  measure?: string;
}

export const api = {
  pickVault: () => invoke<string | null>('pick_vault'),
  checkVaultPermission: (root: string) =>
    invoke<boolean>('check_vault_permission', { root }),
  listDir: (dir: string) => invoke<FileNode[]>('list_dir', { dir }),
  readDoc: (entry: string) => invoke<RenderResult>('read_doc', { entry }),
  /** 读取图片并返回 data URL（Rust 端 base64 编码），前端直接赋给 img.src */
  readImage: (file: string) => invoke<string>('read_image', { file }),
  /** 是否移动端（Android/iOS），用于隐藏桌面专属功能 */
  isMobile: () => invoke<boolean>('is_mobile'),
  /** 导出当前文档为 PDF（Windows 走 WebView2 PrintToPdf）。返回保存路径，取消返回 null */
  exportPdf: (suggestedName?: string) =>
    invoke<string | null>('export_pdf', { suggestedName: suggestedName ?? null }),
  readTempDoc: (uri: string) => invoke<RenderResult>('read_temp_doc', { uri }),
  extractPlain: (entry: string) => invoke<PlainDoc>('extract_plain', { entry }),
  collectMarkdownFiles: (root: string) =>
    invoke<FileNode[]>('collect_markdown_files', { root }),
  buildIndex: (root: string) => invoke<number>('build_index', { root }),
  savePrefs: (prefs: PrefsDto) => invoke('save_prefs', { prefs }),
  loadPrefs: () => invoke<PrefsDto>('load_prefs'),
};
