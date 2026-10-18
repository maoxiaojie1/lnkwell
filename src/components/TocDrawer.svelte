<script lang="ts">
  import { nav } from '../lib/stores/nav.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import { reading } from '../lib/stores/reading.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { addBookmark, listBookmarks } from '../lib/db';
  import type { TocItem } from '../lib/api';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
    /** 侧栏模式下跳转不应关闭面板（目录与正文同层时它本来就是常驻的） */
    closeOnJump?: boolean;
  }
  let { onClose, closeOnJump = true }: Props = $props();

  let toc = $derived(nav.current?.result.toc ?? []);
  let bodyEl: HTMLElement | undefined = $state();
  /** 当前文档已收藏的锚点，用于星标状态 */
  let bookmarked = $state<Set<string>>(new Set());

  function jump(item: TocItem) {
    // 抽屉模式：先关闭再滚动，避免布局抖动；侧栏模式保持展开
    if (closeOnJump) onClose();
    // 委托给 ArticleView：携带目标 chunk 序号，一步渲染到位再滚动
    tocJump.request(item.id, item.chunk);
  }

  // 换文档时刷新星标集合
  $effect(() => {
    const entry = nav.current?.entry;
    const vid = vault.vaultId;
    if (!entry || vid == null) {
      bookmarked = new Set();
      return;
    }
    void (async () => {
      try {
        const list = await listBookmarks(vid);
        bookmarked = new Set(list.filter((b) => b.entry === entry).map((b) => b.anchor));
      } catch {
        /* 书签不可用不影响阅读 */
      }
    })();
  });

  // 当前章节自动滚进视野（只滚面板内部，不打扰正文）
  $effect(() => {
    const id = reading.anchor;
    if (!id || !bodyEl) return;
    const el = bodyEl.querySelector<HTMLElement>(`[data-toc-id="${CSS.escape(id)}"]`);
    el?.scrollIntoView({ block: 'nearest' });
  });

  /** 收藏某一小节（段落级书签，锚点即标题 id，备注默认取标题文本） */
  async function bookmarkItem(item: TocItem, e: MouseEvent) {
    e.stopPropagation();
    const entry = nav.current?.entry;
    const vid = vault.vaultId;
    if (nav.current?.isTemp) {
      vault.showToast('临时分享文档无法收藏');
      return;
    }
    if (!entry || vid == null) {
      vault.showToast('请先选择文档库');
      return;
    }
    if (bookmarked.has(item.id)) {
      vault.showToast('这一节已经收藏过了');
      return;
    }
    try {
      await addBookmark(vid, entry, item.id, item.text);
      bookmarked = new Set([...bookmarked, item.id]);
      vault.showToast(`已收藏「${item.text}」`);
    } catch (err) {
      vault.showError(err);
    }
  }
</script>

<div class="drawer-header">
  <span>目录</span>
  {#if toc.length > 0}
    <span class="outline-count">{toc.length}</span>
  {/if}
  <button class="icon-btn" title={closeOnJump ? '关闭' : '收起目录'} onclick={onClose}>
    <Icon name="close" size={14} />
  </button>
</div>
<div class="drawer-body" bind:this={bodyEl}>
  {#if !nav.current}
    <div class="search-hint">尚未打开文档</div>
  {:else if toc.length === 0}
    <div class="search-hint">本文档没有标题</div>
  {:else}
    {#each toc as item (item.id)}
      <!-- 层级缩进交给 CSS（--toc-depth），不在组件里写死 padding。
           类名用 outline-item：.toc-item 属于文档内 [TOC] 列表，避免互相污染。 -->
      <div class="outline-row">
        <button
          class="outline-item lvl-{item.level}"
          class:active={item.id === reading.anchor}
          style:--toc-depth={Math.min(item.level - 1, 4)}
          data-toc-id={item.id}
          title={item.text}
          onclick={() => jump(item)}
        >{item.text}</button>
        <button
          class="outline-star"
          class:on={bookmarked.has(item.id)}
          title={bookmarked.has(item.id) ? '已收藏' : '收藏此节'}
          onclick={(e) => bookmarkItem(item, e)}
        >
          <Icon name="star" size={12} />
        </button>
      </div>
    {/each}
  {/if}
</div>
