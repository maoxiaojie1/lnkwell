<script lang="ts">
  import { onMount, tick, untrack } from 'svelte';
  import { api, errorMessage } from '../lib/api';
  import { vault } from '../lib/stores/vault.svelte';
  import { reading } from '../lib/stores/reading.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import { parseHeadings, pairHeadings, type HeadingPair } from '../lib/render/source-map';
  import type { OpenDoc } from '../lib/stores/nav.svelte';
  import ArticleView from './ArticleView.svelte';
  import Icon from './Icons.svelte';

  /**
   * 并排对照：左边源码、右边渲染，按**标题**双向同步滚动。
   *
   * 精度说明：同步锚点是标题，不是逐行。行级映射需要 pulldown-cmark 的
   * 字节偏移 + 每个渲染节点的源位置，成本远大于收益；标题级已经能满足
   * 「看到哪一节、那节渲染成什么样」的对照需求（VS Code 的同步也是按元素）。
   */
  interface Props {
    doc: OpenDoc;
    onOpenImage: (src: string) => void;
    onSource: () => void;
    onClose: () => void;
  }
  let { doc, onOpenImage, onSource, onClose }: Props = $props();

  let text = $state<string | null>(null);
  let error = $state<string | null>(null);
  let pairs = $state<HeadingPair[]>([]);

  let sourceScroll: HTMLDivElement | undefined = $state();
  let renderPane: HTMLElement | undefined = $state();
  let lineHeight = $state(22);

  /** 谁在驱动：程序化滚动期间忽略另一侧的 scroll 事件，避免来回拉锯 */
  let lock: 'src' | 'render' | null = null;
  let lockTimer: ReturnType<typeof setTimeout> | undefined;
  let lastIdx = -1;

  let lines = $derived((text ?? '').replace(/\n$/, '').split('\n'));

  function setLock(who: 'src' | 'render', ms = 280) {
    lock = who;
    clearTimeout(lockTimer);
    lockTimer = setTimeout(() => (lock = null), ms);
  }

  function bodyEl(): HTMLElement | null {
    return sourceScroll?.querySelector('.source-body') ?? null;
  }
  function previewScroll(): HTMLElement | null {
    return renderPane?.querySelector('.reader-scroll') ?? null;
  }

  /** 源码正文顶部相对滚动容器内容原点的偏移（offsetTop 会随 offsetParent 漂移，用 rect 算） */
  function bodyTop(): number {
    const body = bodyEl();
    const el = sourceScroll;
    if (!body || !el) return 0;
    return body.getBoundingClientRect().top - el.getBoundingClientRect().top + el.scrollTop;
  }

  /** 源码行高是恒定的（每行一个 div，white-space: pre 不折行），实测一次即可 */
  function measure() {
    const line = sourceScroll?.querySelector('.src-line') as HTMLElement | null;
    if (line) lineHeight = line.getBoundingClientRect().height || lineHeight;
  }

  function lineTop(line: number): number {
    return bodyTop() + line * lineHeight;
  }

  function onSourceScroll() {
    if (lock === 'render' || !sourceScroll) return;
    if (pairs.length === 0) return;

    const line = Math.floor((sourceScroll.scrollTop - bodyTop()) / lineHeight);
    let idx = -1;
    for (let i = 0; i < pairs.length; i++) if (pairs[i].line <= line) idx = i;
    if (idx < 0 || idx === lastIdx) return;

    lastIdx = idx;
    setLock('src');
    // 交给 ArticleView：它负责把目标块渲染出来再滚过去（懒加载由它自己处理）
    tocJump.request(pairs[idx].id, pairs[idx].chunk);
  }

  function onPreviewScroll() {
    if (lock === 'src' || !sourceScroll) return;
    const sc = previewScroll();
    if (!sc) return;

    const rootTop = sc.getBoundingClientRect().top;
    const headings = sc.querySelectorAll<HTMLElement>(
      '.markdown-body h1[id], .markdown-body h2[id], .markdown-body h3[id], .markdown-body h4[id]',
    );
    let current: HTMLElement | null = null;
    for (const h of headings) {
      if (h.getBoundingClientRect().top - rootTop <= 80) current = h;
      else break;
    }
    if (!current) return;

    const idx = pairs.findIndex((p) => p.id === current!.id);
    if (idx < 0 || idx === lastIdx) return;

    lastIdx = idx;
    setLock('render');
    sourceScroll.scrollTo({ top: lineTop(pairs[idx].line) - 8, behavior: 'auto' });
  }

  async function init() {
    try {
      text = await api.readRawDoc(doc.entry);
      pairs = pairHeadings(parseHeadings(text), doc.result.toc);
    } catch (e) {
      error = errorMessage(e);
      return;
    }
    await tick();
    measure();

    // 预览侧是 ArticleView 自己的滚动容器，这里补一个监听做反向同步
    const sc = previewScroll();
    sc?.addEventListener('scroll', onPreviewScroll, { passive: true });
    unlistenPreview = () => sc?.removeEventListener('scroll', onPreviewScroll);

    // 从正文当前章节对齐过去（进入对照时两边对得上）
    const anchor = untrack(() => reading.anchor);
    if (!anchor) return;
    const idx = pairs.findIndex((p) => p.id === anchor);
    if (idx < 0) return;
    lastIdx = idx;
    setLock('src', 400);
    sourceScroll?.scrollTo({ top: lineTop(pairs[idx].line) - 8, behavior: 'auto' });
    tocJump.request(pairs[idx].id, pairs[idx].chunk);
  }

  let unlistenPreview: (() => void) | undefined;

  onMount(init);
  $effect(() => () => unlistenPreview?.());
</script>

<div class="split">
  <div class="split-bar">
    <span class="split-badge">只读</span>
    <span class="split-title">并排对照</span>
    <span class="split-meta">按标题同步 · {pairs.length} 个锚点</span>
    <span class="spacer"></span>
    <button class="split-act" onclick={onSource}>
      <Icon name="code" size={13} />
      仅源码
    </button>
    <button class="split-act" onclick={onClose}>
      <Icon name="close" size={13} />
      返回阅读
    </button>
  </div>

  <div class="split-body">
    <section class="split-pane source-pane" aria-label="Markdown 源码">
      <div class="pane-head">源码</div>
      <div class="pane-scroll" bind:this={sourceScroll} onscroll={onSourceScroll}>
        {#if error}
          <div class="pane-hint">读取失败：{error}</div>
        {:else if text === null}
          <div class="pane-hint">读取中…</div>
        {:else}
          <div class="source-body">
            {#each lines as line, i (i)}
              <div class="src-line"><span class="src-no">{i + 1}</span><span class="src-txt">{line || ' '}</span></div>
            {/each}
          </div>
        {/if}
      </div>
    </section>

    <div class="split-divider" aria-hidden="true"></div>

    <section class="split-pane render-pane" bind:this={renderPane} aria-label="渲染预览">
      <div class="pane-head">渲染预览</div>
      <ArticleView {doc} passive {onOpenImage} />
    </section>
  </div>
</div>

<style>
  .split {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--ui-bg);
  }

  .split-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--ui-border-soft);
    background: var(--ui-bg-alt);
    font-size: 12px;
    color: var(--ui-fg-muted);
  }
  .split-bar .spacer { flex: 1; }
  .split-badge {
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--ui-border);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
  }
  .split-title { color: var(--ui-fg-2); font-weight: 600; }
  .split-meta { font-variant-numeric: tabular-nums; }

  .split-act {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    padding: 4px 10px;
    border: 1px solid var(--ui-border);
    border-radius: var(--ui-r-sm);
    background: var(--ui-bg);
    color: var(--ui-fg-2);
    font-size: 12px;
    cursor: pointer;
    transition: background 0.14s ease, border-color 0.14s ease, color 0.14s ease;
  }
  .split-act:hover {
    background: var(--ui-bg-hover);
    border-color: color-mix(in srgb, var(--ui-accent) 40%, var(--ui-border));
    color: var(--ui-fg);
  }

  .split-body {
    flex: 1;
    min-height: 0;
    display: flex;
    align-items: stretch;
  }
  .split-pane {
    flex: 1 1 0;
    min-width: 0;
    display: flex;
    flex-direction: column;
    min-height: 0;
  }
  .split-pane.render-pane { flex: 1.05 1 0; }

  .split-divider {
    width: 1px;
    flex-shrink: 0;
    background: var(--ui-border-soft);
  }

  .pane-head {
    flex-shrink: 0;
    padding: 6px 14px;
    font-size: 10.5px;
    font-weight: 650;
    letter-spacing: 0.09em;
    text-transform: uppercase;
    color: var(--ui-fg-muted);
    background: var(--ui-bg);
    border-bottom: 1px solid var(--ui-border-soft);
  }

  .pane-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .pane-hint { padding: 20px 16px; font-size: 12.5px; color: var(--ui-fg-muted); }

  .source-body {
    padding: 8px 0 40px;
    font-family: var(--md-font-mono);
    font-size: 12px;
    line-height: 1.75;
    color: var(--ui-fg-2);
    tab-size: 4;
    min-width: max-content;
  }
  .src-line { display: flex; align-items: flex-start; padding-right: 20px; }
  .src-line:hover { background: color-mix(in srgb, var(--ui-fg) 4%, transparent); }
  .src-no {
    flex-shrink: 0;
    width: 44px;
    padding-right: 12px;
    text-align: right;
    color: var(--ui-fg-muted);
    opacity: 0.5;
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .src-txt { white-space: pre; flex: 1; }

  /* 预览侧不要平滑滚动：同步是一跳一跳的，平滑会造成「追不上」的观感 */
  .split :global(.reader-scroll) { scroll-behavior: auto; }
  .split :global(.markdown-body) { padding-left: 28px; padding-right: 28px; }
</style>
