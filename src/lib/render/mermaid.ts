import type { Mermaid } from 'mermaid';

/**
 * Mermaid 懒加载（方案 §5.5）：~1MB 且渲染吃 CPU，
 * 仅当文档包含图表且滚到附近时才加载渲染。
 */

type MermaidModule = typeof import('mermaid');

let mermaidPromise: Promise<MermaidModule> | null = null;
let currentTheme = '';

function loadMermaid(): Promise<MermaidModule> {
  mermaidPromise ??= import('mermaid');
  return mermaidPromise;
}

const cssVar = (n: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(n).trim();

function initWithTheme(mermaid: Mermaid) {
  // Mermaid 渲染成 SVG，不吃 CSS 变量，必须显式喂主题（方案 §6.6）
  mermaid.initialize({
    startOnLoad: false,
    theme: 'base',
    themeVariables: {
      background: cssVar('--md-bg'),
      primaryColor: cssVar('--md-code-bg'),
      primaryTextColor: cssVar('--md-fg'),
      primaryBorderColor: cssVar('--md-border'),
      lineColor: cssVar('--md-border'),
      fontFamily: cssVar('--md-font-body'),
    },
  });
  currentTheme = document.documentElement.dataset.theme ?? '';
}

const observer = new IntersectionObserver(
  (entries) => {
    for (const entry of entries) {
      if (!entry.isIntersecting) continue;
      const el = entry.target as HTMLElement;
      observer.unobserve(el);
      void renderOne(el);
    }
  },
  // 提前 200px 渲染，用户感觉不到延迟
  { rootMargin: '200px' }
);

async function renderOne(el: HTMLElement) {
  // 首次渲染前存原文（方案 §6.6：渲染会用 SVG 覆盖原文本，之后找不回来）
  el.dataset.mermaidSrc ??= el.textContent ?? '';
  try {
    const mod = await loadMermaid();
    const mermaid = mod.default;
    if (currentTheme !== (document.documentElement.dataset.theme ?? '')) {
      initWithTheme(mermaid);
    }
    await mermaid.run({ nodes: [el] });
  } catch (e) {
    console.warn('mermaid render failed', e);
  }
}

/** 观察 scope 内的 mermaid 代码块（增量增强的一部分） */
export function observeMermaidWithin(scope: HTMLElement): void {
  scope.querySelectorAll<HTMLElement>('pre > code.language-mermaid').forEach((code) => {
    const pre = code.parentElement;
    if (!pre || pre.classList.contains('mermaid')) return;
    // 把 <pre><code class="language-mermaid"> 换成 mermaid 可识别的 <pre class="mermaid">
    pre.classList.add('mermaid');
    pre.textContent = code.textContent ?? '';
    observer.observe(pre);
  });
}

/** 切主题后重渲染所有已处理的图表（方案 §6.6） */
export async function refreshMermaidTheme(): Promise<void> {
  const rendered = document.querySelectorAll<HTMLElement>('pre.mermaid[data-processed="true"]');
  if (rendered.length === 0) return;
  const mod = await loadMermaid();
  const mermaid = mod.default;
  initWithTheme(mermaid);
  const nodes: HTMLElement[] = [];
  for (const el of rendered) {
    el.removeAttribute('data-processed');
    el.innerHTML = el.dataset.mermaidSrc ?? '';
    nodes.push(el);
  }
  await mermaid.run({ nodes });
}
