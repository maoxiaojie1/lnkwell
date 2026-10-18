<script lang="ts">
  import { onMount } from 'svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { reading } from '../lib/stores/reading.svelte';
  import {
    listBookmarks,
    removeBookmark,
    addBookmark,
    updateBookmarkNote,
    type Bookmark,
  } from '../lib/db';
  import { displayPath, entryName } from '../lib/display';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let bookmarks = $state<Bookmark[]>([]);
  let loading = $state(true);
  let editingId = $state<number | null>(null);
  let noteDraft = $state('');

  async function refresh() {
    loading = true;
    try {
      if (vault.vaultId != null) {
        bookmarks = await listBookmarks(vault.vaultId);
      }
    } catch (e) {
      vault.showError(e);
    } finally {
      loading = false;
    }
  }

  onMount(refresh);

  function baseName(p: string): string {
    return entryName(p);
  }

  /** 收藏当前阅读位置：锚点取大纲里最靠上的可见标题，备注默认用标题文本 */
  async function bookmarkCurrent() {
    if (!nav.current || nav.current.isTemp) {
      vault.showToast('临时分享文档无法收藏');
      return;
    }
    if (vault.vaultId == null) {
      vault.showToast('请先选择文档库');
      return;
    }
    const anchor = reading.anchor ?? 'doc-top';
    const item = nav.current.result.toc.find((t) => t.id === anchor);
    try {
      await addBookmark(vault.vaultId, nav.current.entry, anchor, item?.text ?? null);
      vault.showToast(item ? `已收藏「${item.text}」` : '已收藏当前文档');
      await refresh();
    } catch (e) {
      vault.showError(e);
    }
  }

  async function openBookmark(b: Bookmark) {
    try {
      await nav.open(b.entry, b.note ?? b.title ?? baseName(b.entry));
      onClose();
      if (b.anchor && b.anchor !== 'doc-top') {
        requestAnimationFrame(() => {
          setTimeout(() => {
            document.getElementById(b.anchor)?.scrollIntoView({ behavior: 'smooth' });
          }, 300);
        });
      }
    } catch (e) {
      vault.showError(e);
    }
  }

  function startEdit(b: Bookmark) {
    editingId = b.id;
    noteDraft = b.note ?? '';
  }

  async function saveNote(b: Bookmark) {
    const note = noteDraft.trim() || null;
    try {
      await updateBookmarkNote(b.id, note);
      bookmarks = bookmarks.map((x) => (x.id === b.id ? { ...x, note } : x));
      editingId = null;
      vault.showToast('备注已保存');
    } catch (e) {
      vault.showError(e);
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
  <button class="primary-btn" style="margin:12px 16px" onclick={bookmarkCurrent}>
    <Icon name="star" size={15} />
    收藏当前阅读位置
  </button>
{/if}

<div class="drawer-body">
  {#if loading}
    <div class="search-hint">加载中…</div>
  {:else if vault.vaultId == null}
    <div class="search-hint">请先选择文档库</div>
  {:else if bookmarks.length === 0}
    <div class="search-hint">
      还没有书签。<br />
      阅读时按 <strong>Ctrl+D</strong>，或在右侧目录里点小节后面的星标。
    </div>
  {:else}
    {#each bookmarks as b (b.id)}
      <div class="bookmark-row">
        <button class="search-result" onclick={() => openBookmark(b)}>
          <div class="r-title">{b.note || b.title || baseName(b.entry)}</div>
          <div class="r-path">{displayPath(b.entry, vault.root)}</div>
          {#if b.note && b.title}
            <div class="r-excerpt">{b.title}</div>
          {/if}
        </button>
        <div class="bookmark-actions">
          <button class="bk-act" title="写备注" onclick={() => startEdit(b)}>备注</button>
          <button class="bk-act" title="删除" onclick={(e) => remove(b, e)}>
            <Icon name="close" size={12} />
          </button>
        </div>
        {#if editingId === b.id}
          <div class="bookmark-note">
            <input
              bind:value={noteDraft}
              placeholder="给这条书签写个备注…"
              onkeydown={(e) => {
                if (e.key === 'Enter') {
                  e.preventDefault();
                  void saveNote(b);
                }
                if (e.key === 'Escape') editingId = null;
              }}
            />
            <button class="secondary-btn" onclick={() => void saveNote(b)}>保存</button>
          </div>
        {/if}
      </div>
    {/each}
  {/if}
</div>
