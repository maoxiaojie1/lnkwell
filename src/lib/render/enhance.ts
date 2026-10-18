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
  decorateCodeBlocks(scope);
  alignNumericColumns(scope);
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
 * 本地图片：`md-rel://` 哨兵（放在 data-src 上，避免浏览器预取）→
 * invoke `read_image_in_doc` 取 data URL，再赋给 img.src。
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
  const imgs = Array.from(
    scope.querySelectorAll<HTMLImageElement>(
      `img[data-src^="${REL_PREFIX}"], img[src^="${REL_PREFIX}"]`,
    ),
  );
  await Promise.all(
    imgs.map(async (img) => {
      const attr = img.getAttribute('data-src') ?? img.getAttribute('src') ?? '';
      const raw = attr.slice(REL_PREFIX.length).split('#')[0];
      let rel = raw;
      try {
        rel = decodeURIComponent(raw);
      } catch {
        /* 非法转义时按原样使用 */
      }
      try {
        img.src = await api.readImageInDoc(docEntry, rel);
        img.removeAttribute('data-src');
      } catch (e) {
        console.error(`[img] 加载失败 ${rel}:`, e);
        img.alt = `${img.alt}（图片加载失败）`;
      }
    }),
  );
}

/** 语言标识 → 展示名（只在常见语言上做映射，其余按原样显示） */
const LANG_LABEL: Record<string, string> = {
  ts: 'TypeScript',
  typescript: 'TypeScript',
  js: 'JavaScript',
  javascript: 'JavaScript',
  jsx: 'JSX',
  tsx: 'TSX',
  py: 'Python',
  python: 'Python',
  sh: 'Shell',
  bash: 'Bash',
  zsh: 'Shell',
  json: 'JSON',
  yaml: 'YAML',
  yml: 'YAML',
  toml: 'TOML',
  rs: 'Rust',
  rust: 'Rust',
  go: 'Go',
  java: 'Java',
  kotlin: 'Kotlin',
  swift: 'Swift',
  c: 'C',
  cpp: 'C++',
  cs: 'C#',
  html: 'HTML',
  css: 'CSS',
  scss: 'SCSS',
  sql: 'SQL',
  md: 'Markdown',
  markdown: 'Markdown',
  tex: 'LaTeX',
  latex: 'LaTeX',
  diff: 'Diff',
  ini: 'INI',
  dockerfile: 'Dockerfile',
};

/** 代码块装饰：左上语言标签 + 右上复制按钮（方案 §7.1） */
function decorateCodeBlocks(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLElement>('pre').forEach((pre) => {
    if (pre.classList.contains('mermaid')) return;
    const code = pre.querySelector('code');
    if (!code) return;

    // 语言标签：从 language-xxx 类名取，纯文本块不显示
    if (!pre.querySelector('.code-lang')) {
      const cls = Array.from(code.classList).find((c) => c.startsWith('language-'));
      const raw = cls ? cls.slice('language-'.length).toLowerCase() : '';
      // hljs 会给没有语言的块写死 language-undefined
      if (raw && !NO_LANG_LABEL.has(raw)) {
        const tag = document.createElement('span');
        tag.className = 'code-lang';
        tag.textContent = LANG_LABEL[raw] ?? raw;
        pre.appendChild(tag);
        pre.classList.add('has-lang');
      }
    }

    if (pre.querySelector('.code-copy-btn')) return;
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

const CJK = /[\u3400-\u9fff]/;

/** 被 hljs 标记为"没有语言"的几种写法，都不该显示标签 */
const NO_LANG_LABEL = new Set(['text', 'plaintext', 'nohighlight', 'undefined']);

/**
 * 单元格的可见文本。
 * KaTeX 会同时输出 MathML（含 LaTeX 源码）和可见字形，直接读 textContent
 * 会拿到 `10^{-4} \sim 0.02` 和 `10−4∼0.02` 的拼接，数值判定必然失败。
 */
function visibleText(cell: HTMLTableCellElement): string {
  if (!cell.querySelector('.katex')) return cell.textContent ?? '';
  const clone = cell.cloneNode(true) as HTMLElement;
  clone.querySelectorAll('.katex-mathml').forEach((n) => n.remove());
  return clone.textContent ?? '';
}

/** 单个单元格是否"像数字"：有数字、无中文、字母占比不高（容得下 ≈N(0,1)、10^{-4}） */
function isNumericCell(text: string): boolean {
  const s = text.replace(/\s+/g, '');
  if (!s || CJK.test(s) || !/\d/.test(s)) return false;
  const letters = (s.match(/[A-Za-z]/g) ?? []).length;
  return letters / s.length <= 0.34;
}

/**
 * 表格数值列右对齐。技术文档里「取值范围 / 参数量」这类列左对齐很难扫读，
 * 但 Markdown 没有列类型信息，只能按内容猜：某列 70% 以上的单元格像数字就右对齐。
 * 混合列（中英文 + 数字）保持左对齐，宁可不动也不要对齐得莫名其妙。
 */
function alignNumericColumns(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLTableElement>('table').forEach((table) => {
    if (table.dataset.numAligned) return;
    table.dataset.numAligned = '1';
    const rows = Array.from(table.rows);
    if (rows.length < 2) return;

    const colCount = Math.max(...rows.map((r) => r.cells.length));
    for (let c = 0; c < colCount; c++) {
      const bodyCells = rows
        .slice(1)
        .map((r) => r.cells[c])
        .filter((cell): cell is HTMLTableCellElement => Boolean(cell));
      if (bodyCells.length === 0) continue;
      const hits = bodyCells.filter((cell) => isNumericCell(visibleText(cell))).length;
      if (hits / bodyCells.length < 0.7) continue;
      for (const row of rows) row.cells[c]?.classList.add('num');
    }
  });
}
