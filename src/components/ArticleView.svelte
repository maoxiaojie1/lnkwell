<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { nav, type OpenDoc } from '../lib/stores/nav.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import { enhance, renderMathAll } from '../lib/render/enhance';
  import { loadProgress, saveProgress, upsertVault } from '../lib/db';
  import { errorMessage } from '../lib/api';

  interface Props {
    doc: OpenDoc;
    onOpenImage: (src: string) => void;
  }
  let { doc, onOpenImage }: Props = $props();

  let articleEl: HTMLElement | undefined = $state();
  let scrollEl: HTMLElement | undefined = $state();
  let renderedCount = $state(0);
  let sentinel: HTMLElement | undefined = $state();
  let vaultId: number | null = null;

  // 首屏只注入 2–3 块，其余由哨兵触发追加（方案 §8.1）
  const FIRST_CHUNK_COUNT = 3;

  let chunkObserver: IntersectionObserver | null = null;

  $effect(() => {
    // 文档切换：重置并从头渲染
    const d = doc;
    renderedCount = 0;
    enhancedBoundary = null;
    void renderDoc(d);
    return () => {
      chunkObserver?.disconnect();
      chunkObserver = null;
    };
  });

  async function renderDoc(d: OpenDoc) {
    await tick();
    if (!articleEl) return;
    articleEl.style.opacity = '0';
    renderedCount = Math.min(FIRST_CHUNK_COUNT, d.result.chunks.length);
    await tick();
    const scope = articleEl;
    await enhance(scope, false);

    // 恢复阅读进度（存 anchor id 不存像素，方案 §8.4）
    try {
      const v = vault.root ? await upsertVault(vault.name, vault.root) : null;
      vaultId = v?.id ?? null;
      const saved = vaultId ? await loadProgress(vaultId, d.entry) : null;
      if (saved?.anchor) {
        // 目标锚点可能尚未渲染：追加分块直到出现再滚动（与 TOC 跳转同一逻辑）
        while (renderedCount < d.result.chunks.length && !document.getElementById(saved.anchor)) {
          await appendMoreChunks(true);
        }
        document.getElementById(saved.anchor)?.scrollIntoView();
      } else if (scrollEl) {
        scrollEl.scrollTop = 0;
      }
    } catch {
      /* 进度恢复失败不影响阅读 */
    }
    // 首屏增强完成后再显示（防公式/高亮跳变，方案 §5.1）
    if (articleEl) articleEl.style.opacity = '1';

    setupChunkObserver();
  }

  function setupChunkObserver() {
    chunkObserver?.disconnect();
    if (!sentinel || !scrollEl) return;
    chunkObserver = new IntersectionObserver(
      (entries) => {
        if (!entries.some((e) => e.isIntersecting)) return;
        if (renderedCount >= doc.result.chunks.length) return;
        void appendMoreChunks();
      },
      { root: scrollEl, rootMargin: '600px' }
    );
    chunkObserver.observe(sentinel);
  }

  // TOC 跳转：目标章节可能尚未渲染。若锚点不在 DOM 则持续追加分块，
  // 直到渲染出来后平滑滚动到位（问题3：长文档目录跳转未渲染章节）。
  // 注意：必须用 untrack 隔离对 renderedCount 的读取——否则每次追加分块
  // 改变 renderedCount 都会触发本 effect 重跑，形成并发竞态，永远滚不动。
  $effect(() => {
    const target = tocJump.target;
    if (!target) return;
    void untrack(async () => {
      // 先快速渲染到包含锚点的分块（不逐块等 enhance 图片，滚动优先）
      while (renderedCount < doc.result.chunks.length && !document.getElementById(target)) {
        await appendMoreChunks(true);
      }
      // 锚点已渲染（或全部渲染完仍找不到），执行滚动并清除
      const el = document.getElementById(target);
      if (el) el.scrollIntoView({ behavior: 'smooth' });
      tocJump.clear();
    });
  });

  // 记录追加前的 DOM，只对新节点跑增强（增量，方案 §5.1 配合规则）
  let enhancedBoundary: ChildNode | null = null;

  // fast=true 时跳过逐块 await（用于 TOC 跳转快速铺开 DOM，图片后台补齐）
  async function appendMoreChunks(fast = false) {
    const before = renderedCount;
    renderedCount = Math.min(renderedCount + 3, doc.result.chunks.length);
    await tick();
    if (!articleEl || renderedCount === before) return;
    // 找到新增的节点范围，只对新节点跑增强
    const nodes = Array.from(articleEl.childNodes);
    const startIdx = enhancedBoundary ? nodes.indexOf(enhancedBoundary) + 1 : 0;
    const newNodes = nodes.slice(Math.max(startIdx, 0)).filter((n) => n !== sentinel);
    const jobs: Promise<void>[] = [];
    for (const n of newNodes) {
      if (n instanceof HTMLElement) {
        const job = enhance(n, false);
        if (fast) jobs.push(job);
        else await job;
      }
    }
    if (fast) void Promise.all(jobs); // 后台完成增强，不阻塞滚动
    if (newNodes.length > 0) enhancedBoundary = newNodes[newNodes.length - 1];
    // 公式可能在任意分块里，且 enhance 的 scope 是单节点会漏掉跨节点的公式。
    // 追加后对整篇补跑一次 renderMath（幂等：已含 .katex 的跳过）。
    if (articleEl) renderMathAll(articleEl);
    // 更新哨兵观察
    setupChunkObserver();
    }

  // 阅读进度保存（防抖）：记录视口顶部最近的标题（方案 §8.4）
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  function onScroll() {
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void persistProgress(), 800);
  }

  async function persistProgress() {
    if (!scrollEl || !articleEl || vaultId == null) return;
    const headings = articleEl.querySelectorAll<HTMLElement>('h1[id], h2[id], h3[id]');
    const rootTop = scrollEl.getBoundingClientRect().top;
    let current: HTMLElement | null = null;
    for (const h of headings) {
      if (h.getBoundingClientRect().top - rootTop <= 10) current = h;
      else break;
    }
    if (!current?.id) return;
    try {
      await saveProgress(vaultId, doc.entry, current.id, 0);
    } catch {
      /* 忽略 */
    }
  }

  /** 点击拦截：md-link:// 转内部导航；外链新开窗口；图片进查看器（方案 §7.1） */
  function onClick(e: MouseEvent) {
    const target = e.target as HTMLElement;

    const img = target.closest('img');
    if (img && articleEl?.contains(img)) {
      e.preventDefault();
      onOpenImage(img.src);
      return;
    }

    const a = target.closest('a');
    if (!a || !articleEl?.contains(a)) return;
    const href = a.getAttribute('href') ?? '';

    if (href.startsWith('md-link://')) {
      e.preventDefault();
      const rel = href.slice('md-link://'.length);
      const [pathPart, hash] = rel.split('#');
      const resolved = resolveRelativePath(doc.entry, pathPart);
      const name = resolved.replace(/\\/g, '/').split('/').pop() ?? resolved;
      nav
        .open(resolved, name)
        .then(() => {
          if (hash) {
            requestAnimationFrame(() =>
              document.getElementById(decodeURIComponent(hash))?.scrollIntoView()
            );
          }
        })
        .catch((err) => vault.showToast(errorMessage(err)));
      return;
    }
    if (href.startsWith('#')) {
      e.preventDefault();
      document
        .getElementById(decodeURIComponent(href.slice(1)))
        ?.scrollIntoView({ behavior: 'smooth' });
      return;
    }
    if (/^https?:\/\//.test(href)) {
      e.preventDefault();
      window.open(href, '_blank', 'noopener');
    }
  }

  /** 相对当前文档解析 md 互跳路径（桌面路径字符串；Android content URI 见 §4.3） */
  export function resolveRelativePath(docEntry: string, rel: string): string {
    if (/^[a-zA-Z]:[\\/]/.test(rel) || rel.startsWith('/')) return rel;
    const sep = docEntry.includes('\\') ? '\\' : '/';
    const dirParts = docEntry.replace(/\\/g, '/').split('/');
    dirParts.pop(); // 去掉文件名
    for (const seg of rel.replace(/\\/g, '/').split('/')) {
      if (seg === '.' || seg === '') continue;
      if (seg === '..') dirParts.pop();
      else dirParts.push(seg);
    }
    return dirParts.join(sep);
  }
</script>

<div class="reader-scroll" bind:this={scrollEl} onscroll={onScroll}>
  <!-- onclick 是对渲染后 HTML 的事件代理（图片→查看器、md 链接→内部导航）。
       实际的 <a>/<img> 是原生可聚焦元素，键盘用户 Tab+Enter 不受影响。 -->
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_noninteractive_element_interactions -->
  <article
    class="markdown-body"
    style="opacity: 0"
    bind:this={articleEl}
    onclick={onClick}
    role="document"
  >
    {#each doc.result.chunks.slice(0, renderedCount) as chunk, i (i)}
      {@html chunk}
    {/each}
    <div bind:this={sentinel}></div>
  </article>
</div>
