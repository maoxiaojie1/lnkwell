import { writeText } from '@tauri-apps/plugin-clipboard-manager';
import { api } from '../api';
import { highlightWithin } from './highlight';
import { renderMath } from './katex';
import { observeMermaidWithin } from './mermaid';

/**
 * 增强管线（方案 §5.1）。
 * 必须支持增量调用：后续分块只对新追加的节点跑，不能重扫全文，
 * 否则追加到第 10 块时会重复处理前 9 块，越滚越卡。
 *
 * `docEntry` 是当前文档的条目引用，用于解析文档内相对图片（改进计划 A1）。
 */
export async function enhance(scope: HTMLElement, docEntry: string): Promise<void> {
  await rewriteLocalImages(scope, docEntry);
  highlightWithin(scope);
  renderMath(scope);
  observeMermaidWithin(scope);
  addCopyButtons(scope);
}

/**
 * 对整篇容器补跑公式渲染（幂等）。
 * 分块增强时 scope 是单个节点，跨节点的公式会漏掉；
 * 在追加完所有可见分块后调用此函数，确保所有公式都被渲染。
 */
export function renderMathAll(scope: HTMLElement): void {
  renderMath(scope);
}

/**
 * 本地图片：`md-rel://` 哨兵 → invoke `read_image_in_doc` 取 data URL，直接赋给 img.src。
 *
 * 路径解析**只在 Rust 侧做一次**（改进计划 A1）：前端不再拼路径，
 * 桌面端由 `std::path` 解析，Android 端由 SAF `resolve_file_uri` 解析。
 *
 * 注意：pulldown-cmark 输出 img src 时会把非 ASCII 字符 percent-encode
 * （escape_href），因此这里必须 decodeURIComponent 还原真实路径
 * （中文目录/文件名才能命中磁盘文件）。
 */
const REL_PREFIX = 'md-rel://';

async function rewriteLocalImages(scope: HTMLElement, docEntry: string): Promise<void> {
  const imgs = Array.from(scope.querySelectorAll<HTMLImageElement>(`img[src^="${REL_PREFIX}"]`));
  await Promise.all(
    imgs.map(async (img) => {
      const raw = img.getAttribute('src')!.slice(REL_PREFIX.length).split('#')[0];
      let rel = raw;
      try {
        rel = decodeURIComponent(raw);
      } catch {
        /* 非法转义时按原样使用 */
      }
      try {
        img.src = await api.readImageInDoc(docEntry, rel);
      } catch (e) {
        console.error(`[img] 加载失败 ${rel}:`, e);
        img.alt = `${img.alt}（图片加载失败）`;
      }
    }),
  );
}

/** 代码块复制按钮（方案 §7.1） */
function addCopyButtons(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLElement>('pre').forEach((pre) => {
    if (pre.classList.contains('mermaid')) return;
    if (pre.querySelector('.code-copy-btn')) return;
    const code = pre.querySelector('code');
    if (!code) return;
    const btn = document.createElement('button');
    btn.className = 'code-copy-btn';
    btn.textContent = '复制';
    btn.addEventListener('click', async (e) => {
      e.stopPropagation();
      try {
        await writeText(code.textContent ?? '');
        btn.textContent = '已复制';
      } catch {
        btn.textContent = '失败';
      }
      setTimeout(() => (btn.textContent = '复制'), 1500);
    });
    pre.appendChild(btn);
  });
}
