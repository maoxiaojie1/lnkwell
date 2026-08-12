import katex from 'katex';

/**
 * 渲染数学公式（方案 §5.3）。
 *
 * pulldown-cmark 的 ENABLE_MATH 把 $...$ / $$...$$ 解析为：
 *   <span class="math math-inline">E = mc^2</span>
 *   <span class="math math-display">\int_0^1 x dx</span>
 *
 * 注意：分块增强时 scope 是「单个新节点」，公式可能在另一节点里，
 * 因此 renderMath 向上找 .markdown-body 整篇扫描，幂等跳过已渲染的公式。
 */
export function renderMath(scope: HTMLElement): void {
  try {
    const root =
      scope.closest<HTMLElement>('.markdown-body') ??
      document.querySelector<HTMLElement>('.markdown-body') ??
      scope;

    const spans = root.querySelectorAll<HTMLElement>('span.math');
    let count = 0;
    spans.forEach((span) => {
      if (span.querySelector('.katex')) return; // 已渲染过，跳过
      const src = (span.textContent ?? '').trim();
      if (!src) return;
      const display = span.classList.contains('math-display');
      try {
        katex.render(src, span, { displayMode: display, throwOnError: false });
        count++;
      } catch {
        /* 单个公式失败不影响其他，保留原始源码文本 */
      }
    });
  } catch {
    /* 整体失败不阻断渲染 */
  }
}
