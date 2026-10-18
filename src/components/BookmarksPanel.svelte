<script lang="ts">
  import { onMount } from 'svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { listBookmarks, removeBookmark, addBookmark, type Bookmark } from '../lib/db';
  import { errorMessage } from '../lib/api';
  import { displayPath, entryName } from '../lib/display';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let bookmarks = $state<Bookmark[]>([]);
  let loading = $state(true);

  async function refresh() {
    loading = true;
    try {
      if (vault.vaultId != null) {
        bookmarks = await listBookmarks(vault.vaultId);
      }
    } catch (e) {
      vault.showToast(errorMessage(e));
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  function baseName(p: string): string {
    return entryName(p);
  }

  /** 收藏当前打开的文档（方案 §7.2：文档级收藏，anchor 为文档顶部） */
  async function bookmarkCurrent() {
    if (!nav.current || nav.current.isTemp) {
      vault.showToast('临时分享文档无法收藏');
      return;
    }
    if (vault.vaultId == null) {
      vault.showToast('请先选择文档库');
      return;
    }
    try {
      await addBookmark(vault.vaultId, nav.current.entry, 'doc-top');
      vault.showToast('已收藏');
      await refresh();
    } catch (e) {
      vault.showToast(errorMessage(e));
    }
  }

  async function openBookmark(b: Bookmark) {
    try {
      await nav.open(b.entry, b.title ?? baseName(b.entry));
      onClose();
      if (b.anchor && b.anchor !== 'doc-top') {
        requestAnimationFrame(() => {
          setTimeout(() => {
            document.getElementById(b.anchor)?.scrollIntoView({ behavior: 'smooth' });
          }, 300);
        });
      }
    } catch (e) {
      vault.showToast(errorMessage(e));
    }
  }

  async function remove(b: Bookmark, e: MouseEvent) {
    e.stopPropagation();
    await removeBookmark(b.id);
    bookmarks = bookmarks.filter((x) => x.id !== b.id);
  }
</script>

<div class="drawer-header">
  <span>书签</span>
  <button class="icon-btn" title="关闭" onclick={onClose}><Icon name="close" size={14} /></button>
</div>

{#if nav.current && !nav.current.isTemp}
  <button class="primary-btn" style="margin:10px 14px" onclick={bookmarkCurrent}>
    收藏当前文档
  </button>
{/if}

<div class="drawer-body">
  {#if loading}
    <div class="search-hint">加载中…</div>
  {:else if vault.vaultId == null}
    <div class="search-hint">请先选择文档库</div>
  {:else if bookmarks.length === 0}
    <div class="search-hint">还没有书签。阅读时点「收藏当前文档」添加。</div>
  {:else}
    {#each bookmarks as b (b.id)}
      <button class="search-result" onclick={() => openBookmark(b)}>
        <div class="r-title">{b.title ?? baseName(b.entry)}</div>
        <div class="r-path">{displayPath(b.entry, vault.root)}</div>
        {#if b.note}
          <div class="r-excerpt">{b.note}</div>
        {/if}
        <span class="bookmark-delete" title="删除" role="button" tabindex="0"
          onclick={(e) => remove(b, e)} onkeydown={(e) => e.key === 'Enter' && remove(b, e as unknown as MouseEvent)}><Icon name="close" size={12} /></span>
      </button>
    {/each}
  {/if}
</div>

<style>
  .bookmark-delete {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 24px;
    height: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    border-radius: 6px;
    cursor: pointer;
    color: var(--ui-fg-muted);
    transition: background 0.15s ease, color 0.15s ease;
  }
  .bookmark-delete:hover { background: var(--ui-bg-hover); color: var(--ui-fg); }
</style>
