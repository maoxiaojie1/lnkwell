<script lang="ts">
  import { tick, untrack } from 'svelte';
  import { openUrl } from '@tauri-apps/plugin-opener';
  import { nav, type OpenDoc } from '../lib/stores/nav.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import { find } from '../lib/stores/find.svelte';
  import { reading } from '../lib/stores/reading.svelte';
  import { exportRequest } from '../lib/stores/export.svelte';
  import { enhance, renderMathAll } from '../lib/render/enhance';
  import { applyExportStyle } from '../lib/render/exportStyle';
  import { loadProgress, saveProgress, upsertVault, addBookmark } from '../lib/db';
  import { api } from '../lib/api';
  import Icon from './Icons.svelte';

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

  // front matter 卡片（方案 §7.2）：有 title/date/tags 任一才渲染，
  // 否则返回 null 让模板用 {#if fm} 直接收窄非空类型
  let fm = $derived.by(() => {
    const m = doc.result.frontmatter;
    return m && (m.title || m.date || m.tags.length > 0) ? m : null;
  });

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
    reading.reset();
    articleEl.style.opacity = '0';
    renderedCount = Math.min(FIRST_CHUNK_COUNT, d.result.chunks.length);
    await tick();
    const scope = articleEl;
    try {
      await enhance(scope, d.entry);
      // 记录首屏增强边界：后续 appendMoreChunks 只处理新节点，
      // 避免对整个 DOM 重复增强（图片并发 invoke 风暴会拖慢 TOC 跳转）
      const initialNodes = Array.from(articleEl.childNodes).filter((n) => n !== sentinel);
      if (initialNodes.length > 0) enhancedBoundary = initialNodes[initialNodes.length - 1];

      // 恢复阅读进度（存 anchor id 不存像素，方案 §8.4）
      try {
        const v = vault.root ? await upsertVault(vault.name, vault.root) : null;
        vaultId = v?.id ?? null;
        const saved = vaultId ? await loadProgress(vaultId, d.entry) : null;
        if (saved?.anchor) {
          // 目标锚点可能尚未渲染：按 TOC 项查找其 chunk，一步渲染到位再滚动
          const tocItem = d.result.toc.find((t) => t.id === saved.anchor);
          const targetChunk = tocItem ? tocItem.chunk : -1;
          if (targetChunk >= 0 && renderedCount <= targetChunk) {
            renderedCount = Math.min(targetChunk + 1, d.result.chunks.length);
            await tick();
            const scope = articleEl;
            if (scope) {
              const nodes = Array.from(scope.childNodes);
              const startIdx = enhancedBoundary ? nodes.indexOf(enhancedBoundary) + 1 : 0;
              const newNodes = nodes
                .slice(Math.max(startIdx, 0))
                .filter((n): n is HTMLElement => n instanceof HTMLElement && n !== sentinel);
              void Promise.all(newNodes.map((n) => enhance(n, d.entry)));
              if (newNodes.length > 0) enhancedBoundary = newNodes[newNodes.length - 1];
              renderMathAll(scope);
            }
          }
          document.getElementById(saved.anchor)?.scrollIntoView();
        } else if (scrollEl) {
          scrollEl.scrollTop = 0;
        }
      } catch {
        /* 进度恢复失败不影响阅读 */
      }
    } finally {
      // 必须放在 finally：正文首屏是 opacity:0，一旦增强/恢复抛错而不复位，
      // 用户会看到一片空白（CSP 修好后这条内联样式才真正生效）。
      if (articleEl) articleEl.style.opacity = '1';
    }

    // 初始化顶栏进度线与大纲高亮
    updateReadingState();
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

  // TOC 跳转：后端已给每个标题标注 chunk 序号，直接一步渲染到目标块
  // （renderedCount = chunk + 1），再滚动到锚点。跨度再大也只需一次渲染。
  // untrack 隔离 renderedCount 读取，避免写回触发本 effect 自激重跑。
  $effect(() => {
    const target = tocJump.target;
    if (!target) return;
    void untrack(async () => {
      const { id, chunk } = target;
      // 一步渲染到目标块（含）
      if (chunk >= 0 && renderedCount <= chunk) {
        renderedCount = Math.min(chunk + 1, doc.result.chunks.length);
        await tick();
        const scope = articleEl;
        if (scope) {
          const nodes = Array.from(scope.childNodes);
          const startIdx = enhancedBoundary ? nodes.indexOf(enhancedBoundary) + 1 : 0;
          const newNodes = nodes
            .slice(Math.max(startIdx, 0))
            .filter((n): n is HTMLElement => n instanceof HTMLElement && n !== sentinel);
          // 批量增强（含图片 data URL 异步加载），不阻塞滚动
          void Promise.all(newNodes.map((n) => enhance(n, doc.entry)));
          if (newNodes.length > 0) enhancedBoundary = newNodes[newNodes.length - 1];
          if (scope) renderMathAll(scope);
        }
        setupChunkObserver();
      }
      // 等一帧让新块布局稳定后滚动
      requestAnimationFrame(() => {
        const el = document.getElementById(id);
        if (el) el.scrollIntoView({ behavior: 'smooth' });
        updateReadingState();
        tocJump.clear();
      });
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
        const job = enhance(n, doc.entry);
        if (fast) jobs.push(job);
        else await job;
      }
    }
    if (fast) void Promise.all(jobs); // 后台完成增强，不阻塞滚动
    if (newNodes.length > 0) enhancedBoundary = newNodes[newNodes.length - 1];
    // 公式可能在任意分块里，且 enhance 的 scope 是单节点会漏掉跨节点的公式。
    // 追加后对整篇补跑一次 renderMath（幂等：已含 .katex 的跳过）。
    if (articleEl) renderMathAll(articleEl);
    // 懒加载改变了文档总高度：重算进度线与大纲高亮，否则两者会停在旧值
    updateReadingState();
    // 更新哨兵观察
    setupChunkObserver();
    }

  // 阅读进度保存（防抖）：记录视口顶部最近的标题（方案 §8.4）
  let saveTimer: ReturnType<typeof setTimeout> | undefined;
  let rafPending = false;

  function onScroll() {
    // 高频滚动：每帧最多更新一次瞬时状态（顶栏进度线 + 大纲高亮）
    if (!rafPending) {
      rafPending = true;
      requestAnimationFrame(() => {
        rafPending = false;
        updateReadingState();
      });
    }
    // 落库仍走 800ms 防抖
    clearTimeout(saveTimer);
    saveTimer = setTimeout(() => void persistProgress(), 800);
  }

  /** 更新瞬时阅读状态：滚动进度 + 当前章节锚点 */
  function updateReadingState() {
    if (!scrollEl || !articleEl) return;
    const headings = articleEl.querySelectorAll<HTMLElement>('h1[id], h2[id], h3[id], h4[id]');
    const rootTop = scrollEl.getBoundingClientRect().top;
    let current: HTMLElement | null = null;
    for (const h of headings) {
      if (h.getBoundingClientRect().top - rootTop <= 80) current = h;
      else break;
    }
    const anchorId = current?.id ?? null;
    reading.anchor = anchorId;

    // 进度按「当前章节在目录中的位置」计：
    // 懒加载会让文档高度不断增长，用滚动像素比算出来的值在追加分块后会瞬间失真
    //（续读时尤其明显：只剩 3 个分块时滚到第 4 节会显示 60%）。
    const toc = doc.result.toc;
    if (toc.length > 0) {
      const idx = anchorId ? toc.findIndex((t) => t.id === anchorId) : -1;
      reading.progress = idx >= 0 ? Math.min(1, (idx + 1) / toc.length) : 0;
    } else {
      // 没有标题的文档退化为像素比例（此时内容一次性渲染，比例是准的）
      const max = scrollEl.scrollHeight - scrollEl.clientHeight;
      reading.progress = max > 8 ? Math.min(1, Math.max(0, scrollEl.scrollTop / max)) : 0;
    }
  }

  async function persistProgress() {
    if (vaultId == null) return;
    updateReadingState();
    const anchor = reading.anchor;
    if (!anchor) return;
    try {
      await saveProgress(vaultId, doc.entry, anchor, 0);
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

    if (href.startsWith('md-toc://')) {
      // 文档内 [TOC] 目录：走 tocJump 懒加载跳转（与右侧目录抽屉一致）
      e.preventDefault();
      const id = decodeURIComponent(href.slice('md-toc://'.length));
      const item = doc.result.toc.find((t) => t.id === id);
      tocJump.request(id, item ? item.chunk : -1);
      return;
    }
    if (href.startsWith('md-link://')) {
      e.preventDefault();
      // 与图片同理：pulldown-cmark 输出时对非 ASCII 做了 percent-encode，需解码
      const rel = decodeURIComponent(href.slice('md-link://'.length));
      const [pathPart, hash] = rel.split('#');
      // 路径解析交给 Rust（改进计划 A1）：桌面端走 std::path，Android 走 SAF resolve_file_uri。
      // 前端此前自己 split('/') 拼路径，对 Android 的 FsUri JSON 会产生垃圾路径。
      void (async () => {
        try {
          const entry = await api.resolveEntry(doc.entry, pathPart);
          const name =
            pathPart.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? pathPart;
          await nav.open(entry, name);
          if (hash) {
            requestAnimationFrame(() =>
              document.getElementById(decodeURIComponent(hash))?.scrollIntoView()
            );
          }
        } catch (err) {
          vault.showError(err);
        }
      })();
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
      // Tauri WebView 会吞掉 window.open / target=_blank（改进计划 A10），
      // 必须走 opener 插件交给系统浏览器
      void openUrl(href).catch((err) => vault.showError(err));
    }
  }

  /* ===== 文档内查找（Ctrl+F，方案 §7.1 第一梯队） ===== */
  let findQuery = $state('');
  let findMarks = $state<HTMLElement[]>([]);
  let findIndex = $state(-1);
  let findTotal = $state(0);
  let findInput: HTMLInputElement | undefined = $state();
  let findDebounce: ReturnType<typeof setTimeout> | undefined;

  // 单一飞行：全文渲染可能被多个入口并发触发（打开查找 + 输入查询），
  // 用共享 promise 避免 renderedCount 竞态。
  let renderAllPromise: Promise<void> | null = null;
  function ensureFullyRendered(): Promise<void> {
    if (!renderAllPromise) {
      renderAllPromise = (async () => {
        while (renderedCount < doc.result.chunks.length) {
          await appendMoreChunks(true);
        }
      })().finally(() => {
        renderAllPromise = null;
      });
    }
    return renderAllPromise;
  }

  function clearHighlights() {
    // 仅在确实有高亮时清空，避免对空数组重复赋新引用（触发其他 effect）
    if (findMarks.length === 0) return;
    for (const m of findMarks) {
      const parent = m.parentNode;
      if (parent) {
        m.replaceWith(document.createTextNode(m.textContent ?? ''));
        parent.normalize();
      }
    }
    findMarks = [];
  }

  function scrollToMatch(i: number) {
    const mark = findMarks[i];
    if (mark) mark.scrollIntoView({ block: 'center' });
  }

  async function doFind() {
    const q = findQuery.trim();
    if (!q || !articleEl) {
      clearHighlights();
      findTotal = 0;
      findIndex = -1;
      return;
    }
    // 只在实际有查询词时才渲染全文（懒加载不被破坏）
    await ensureFullyRendered();
    clearHighlights();
    const walker = document.createTreeWalker(articleEl, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        const el = (node as Text).parentElement;
        // 跳过我们已生成的高亮 mark 内部，避免重复命中
        if (el && el.closest('mark.find-mark')) return NodeFilter.FILTER_REJECT;
        // 跳过 front matter 卡片（元数据不属于正文查找范围）
        if (el && el.closest('[data-find-skip]')) return NodeFilter.FILTER_REJECT;
        // 不可见的内容（例如大纲展开时被隐藏的文内 [TOC]）不参与查找，
        // 否则计数里会有一堆点了却跳不过去的命中
        if (!el || el.offsetParent === null) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });
    const textNodes: Text[] = [];
    while (walker.nextNode()) textNodes.push(walker.currentNode as Text);

    const lowerQ = q.toLowerCase();
    const marks: HTMLElement[] = [];
    for (const node of textNodes) {
      const text = node.textContent ?? '';
      if (!text.toLowerCase().includes(lowerQ)) continue;
      const frag = document.createDocumentFragment();
      let idx = 0;
      let pos = text.toLowerCase().indexOf(lowerQ, idx);
      while (pos !== -1) {
        if (pos > idx) frag.appendChild(document.createTextNode(text.slice(idx, pos)));
        const mark = document.createElement('mark');
        mark.className = 'find-mark';
        mark.textContent = text.slice(pos, pos + q.length);
        frag.appendChild(mark);
        marks.push(mark);
        idx = pos + q.length;
        pos = text.toLowerCase().indexOf(lowerQ, idx);
      }
      if (idx < text.length) frag.appendChild(document.createTextNode(text.slice(idx)));
      node.parentNode?.replaceChild(frag, node);
    }
    findMarks = marks;
    findTotal = marks.length;
    findIndex = marks.length > 0 ? 0 : -1;
    if (marks.length > 0) scrollToMatch(0);
  }

  function goNext() {
    if (findTotal === 0) return;
    findIndex = (findIndex + 1) % findTotal;
    scrollToMatch(findIndex);
  }

  function goPrev() {
    if (findTotal === 0) return;
    findIndex = (findIndex - 1 + findTotal) % findTotal;
    scrollToMatch(findIndex);
  }

  // 当前匹配项高亮（更醒目）
  $effect(() => {
    findMarks.forEach((m, i) => m.classList.toggle('find-current', i === findIndex));
  });

  // 查询变化 → 防抖重新查找（仅在查找栏打开时生效）
  $effect(() => {
    if (!find.open) return;
    void findQuery; // 建立对查询词的响应式依赖
    clearTimeout(findDebounce);
    findDebounce = setTimeout(() => void doFind(), 160);
    return () => clearTimeout(findDebounce);
  });

  // 打开/关闭查找栏的生命周期
  $effect(() => {
    if (find.open) {
      tick().then(() => {
        findInput?.focus();
        findInput?.select();
      });
    } else {
      clearHighlights();
      findQuery = '';
    }
  });

  // 文档切换时清空上一文档的查找状态。
  // 必须用 untrack 隔离：clearHighlights 会写 findMarks（新数组引用），
  // 若不加隔离，findMarks 变化会反向触发本 effect，形成死循环（界面卡死）。
  $effect(() => {
    const d = doc;
    void d;
    untrack(() => {
      clearHighlights();
      findQuery = '';
    });
  });

  // 导出 PDF 前的准备：渲染全文 + 等所有图片就绪。
  // 懒加载只铺开了部分分块，PrintToPdf 快照的是当前 DOM，未渲染的分块会缺失，
  // 因此先全量渲染，再等图片（md-file:// 哨兵替换为 data URL 并解码完成）。
  async function prepareForExport(): Promise<void> {
    await ensureFullyRendered();
    if (!articleEl) return;
    // 兜底等待：覆盖之前滚动时以 fast 模式后台加载的图片
    const deadline = Date.now() + 15_000; // 最多 15 秒
    while (Date.now() < deadline) {
      const left = articleEl.querySelectorAll('img[src^="md-file://"]').length;
      if (left === 0) break;
      await new Promise<void>((r) => setTimeout(r, 80));
    }
    const imgs = Array.from(articleEl.querySelectorAll<HTMLImageElement>('img'));
    await Promise.all(imgs.map((img) => img.decode().catch(() => {})));
  }

  // 导出 PDF（方案 §7.2）：仅在 pending 计数增长时触发（首次挂载跳过）。
  // 渲染全文 + 等图片就绪后，注入导出样式（白底/隐藏 UI/排版），
  // 再交 Rust 端走 WebView2 PrintToPdf（无打印对话框、无默认页脚 URL）。
  let lastExportPending = 0;
  $effect(() => {
    const p = exportRequest.pending;
    if (p === lastExportPending) return; // 首次 p=0 与初始值相等，跳过；避免每次打开文档都全量渲染+导出
    lastExportPending = p;
    void untrack(async () => {
      // 先注入导出样式：PrintToPdf 快照屏幕内容，不保证应用 @media print，
      // 用内联样式强制白底 + 隐藏 UI，消除深色主题下的大黑边。
      const cleanup = applyExportStyle();
      try {
        await prepareForExport();
        await tick();
        // 等一帧让全部块布局稳定、注入样式生效
        await new Promise<void>((r) => requestAnimationFrame(() => r()));
        const base = doc.name.replace(/\.(md|markdown)$/i, '');
        const saved = await api.exportPdf(`${base}.pdf`);
        if (saved) vault.showToast(`已导出：${saved}`);
      } catch (e) {
        vault.showError(e);
      } finally {
        cleanup();
      }
    });
  });

  // 全局快捷键：Ctrl/Cmd+F 打开、F3 / Shift+F3 上下跳转、Ctrl/Cmd+D 收藏当前位置
  function onFindKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;
    if (mod && e.key.toLowerCase() === 'f') {
      e.preventDefault();
      find.show();
    } else if (mod && e.key.toLowerCase() === 'd') {
      e.preventDefault();
      void bookmarkHere();
    } else if (e.key === 'F3') {
      e.preventDefault();
      if (e.shiftKey) goPrev();
      else goNext();
    }
  }

  /** 收藏当前阅读位置（段落级书签，锚点取大纲当前项） */
  async function bookmarkHere() {
    if (doc.isTemp) {
      vault.showToast('临时分享文档无法收藏');
      return;
    }
    if (vaultId == null) {
      vault.showToast('请先选择文档库');
      return;
    }
    const anchor = reading.anchor ?? 'doc-top';
    const item = doc.result.toc.find((t) => t.id === anchor);
    try {
      await addBookmark(vaultId, doc.entry, anchor, item?.text ?? null);
      vault.showToast(item ? `已收藏「${item.text}」` : '已收藏当前文档');
    } catch (e) {
      vault.showError(e);
    }
  }

  function onFindInputKeydown(e: KeyboardEvent) {
    if (e.key === 'Enter') {
      e.preventDefault();
      if (e.shiftKey) goPrev();
      else goNext();
    }
  }
</script>

<svelte:window onkeydown={onFindKeydown} />

<div class="reader-scroll" bind:this={scrollEl} onscroll={onScroll}>
  {#if find.open}
    <div class="find-bar" role="search">
      <div class="find-input-wrap">
        <Icon name="find" size={14} />
        <input
          bind:this={findInput}
          bind:value={findQuery}
          placeholder="在文档中查找"
          aria-label="查找"
          onkeydown={onFindInputKeydown}
        />
      </div>
      <span class="find-count">
        {findTotal > 0 ? `${findIndex + 1} / ${findTotal}` : findQuery ? '无结果' : ''}
      </span>
      <button
        class="icon-btn"
        title="上一个 (Shift+F3)"
        onclick={goPrev}
        disabled={findTotal === 0}
      ><Icon name="up" size={15} /></button>
      <button
        class="icon-btn"
        title="下一个 (F3)"
        onclick={goNext}
        disabled={findTotal === 0}
      ><Icon name="down" size={15} /></button>
      <button class="icon-btn" title="关闭 (Esc)" onclick={() => find.close()}>
        <Icon name="close" size={14} />
      </button>
    </div>
  {/if}

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
    {#if fm}
      <div class="frontmatter-card" data-find-skip>
        {#if fm.title}
          <h1 class="fm-title">{fm.title}</h1>
        {/if}
        {#if fm.date}
          <div class="fm-date">{fm.date}</div>
        {/if}
        {#if fm.tags.length > 0}
          <div class="fm-tags">
            {#each fm.tags as tag (tag)}
              <span class="fm-tag">{tag}</span>
            {/each}
          </div>
        {/if}
      </div>
    {/if}
    {#each doc.result.chunks.slice(0, renderedCount) as chunk, i (i)}
      {@html chunk}
    {/each}
    <div bind:this={sentinel}></div>
  </article>
</div>
