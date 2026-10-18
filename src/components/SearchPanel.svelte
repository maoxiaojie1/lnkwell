<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { search } from '../lib/stores/search.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { displayPath, entryName } from '../lib/display';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let inputEl: HTMLInputElement | undefined = $state();
  let listEl: HTMLElement | undefined = $state();
  let debounce: ReturnType<typeof setTimeout> | undefined;

  onMount(async () => {
    await tick();
    // 打开即输入：Ctrl+K 之后不该还要再点一次
    inputEl?.focus();
    inputEl?.select();
    // 保留上次查询：结果还在就不重跑
    if (search.query.trim() && !search.searched) void runSearch();
  });

  function onInput() {
    clearTimeout(debounce);
    debounce = setTimeout(() => void runSearch(), 240);
  }

  async function runSearch() {
    try {
      await search.run();
      await tick();
      scrollActive();
    } catch (e) {
      vault.showError(e);
    }
  }

  function scrollActive() {
    listEl?.querySelector('.search-result.active')?.scrollIntoView({ block: 'nearest' });
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      search.move(1);
      scrollActive();
    } else if (e.key === 'ArrowUp') {
      e.preventDefault();
      search.move(-1);
      scrollActive();
    } else if (e.key === 'Enter') {
      e.preventDefault();
      void openIndex(search.activeIndex);
    }
  }

  async function openIndex(i: number) {
    const hit = search.results[i];
    if (!hit) return;
    await openHit(hit);
  }

  async function openHit(hit: (typeof search.results)[number]) {
    try {
      search.lastOpenedEntry = hit.entry;
      await nav.open(hit.entry, hit.title ?? entryName(hit.entry));
      onClose();
    } catch (e) {
      vault.showError(e);
    }
  }

  /**
   * snippet() 用哨兵串 \u0001/\u0002 包裹命中词；LIKE 兜底没有哨兵。
   * 必须先转义正文再插入标记，否则文档里的 <script> 会经搜索结果注入。
   */
  function excerptHtml(excerpt: string, query: string): string {
    const escaped = excerpt
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
    if (escaped.includes('\u0001')) {
      return escaped.replaceAll('\u0001', '<mark>').replaceAll('\u0002', '</mark>');
    }
    // 短词兜底：前端按查询词补高亮（查询词先转义，避免正则注入）
    const q = query.trim();
    if (!q) return escaped;
    const pattern = q.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
    try {
      return escaped.replace(new RegExp(pattern, 'gi'), (m) => `<mark>${m}</mark>`);
    } catch {
      return escaped;
    }
  }
</script>

<div class="drawer-header">
  <span>搜索</span>
  <button class="icon-btn" title="关闭 (Esc)" onclick={onClose}><Icon name="close" size={14} /></button>
</div>

<div class="search-input-row">
  <input
    class="search-input"
    type="search"
    placeholder="搜索文档内容…"
    bind:this={inputEl}
    bind:value={search.query}
    oninput={onInput}
    onkeydown={onKeydown}
    aria-label="搜索文档内容"
  />
  <div class="search-meta">
    {#if search.fuzzy}
      <span>1–2 个字：使用模糊匹配，可能较慢</span>
    {:else}
      <span class="kbd-hint">↑↓ 选择 · Enter 打开 · Esc 关闭</span>
    {/if}
  </div>
</div>

{#if vault.indexing && vault.indexProgress}
  <div class="index-banner">
    正在建立索引… {vault.indexProgress.done}/{vault.indexProgress.total}
  </div>
{/if}

<div class="drawer-body" bind:this={listEl}>
  {#if !vault.root}
    <div class="search-hint">请先选择文档库</div>
  {:else if search.searching}
    <div class="search-hint">搜索中…</div>
  {:else if !search.query.trim()}
    <div class="search-hint">输入关键词开始搜索</div>
  {:else if search.searched && search.results.length === 0}
    <div class="search-hint">
      没有匹配的结果。
      {#if !vault.indexing}
        <button class="secondary-btn" style="margin-top:8px" onclick={() => vault.startIndexing(true)}>
          重新建立索引
        </button>
      {/if}
    </div>
  {:else}
    {#each search.results as hit, i (hit.doc_id)}
      <button
        class="search-result"
        class:active={i === search.activeIndex}
        onclick={() => openHit(hit)}
        onmouseenter={() => (search.activeIndex = i)}
      >
        <div class="r-title">
          {hit.title ?? entryName(hit.entry)}
          {#if hit.entry === search.lastOpenedEntry}
            <span class="r-flag">上次打开</span>
          {/if}
        </div>
        <div class="r-path">{displayPath(hit.entry, vault.root)}</div>
        <!-- excerpt 已先做 HTML 转义再插入 <mark>，见 excerptHtml -->
        <div class="r-excerpt">{@html excerptHtml(hit.excerpt, search.query)}</div>
      </button>
    {/each}
  {/if}
</div>
