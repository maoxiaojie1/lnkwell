<script lang="ts">
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { searchDocs, upsertVault, type SearchHit } from '../lib/db';
  import { errorMessage } from '../lib/api';
  import { displayPath, entryName } from '../lib/display';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let query = $state('');
  let results = $state<SearchHit[]>([]);
  let searching = $state(false);
  let searched = $state(false);

  let debounce: ReturnType<typeof setTimeout> | undefined;
  function onInput() {
    clearTimeout(debounce);
    debounce = setTimeout(() => void runSearch(), 300);
  }

  async function runSearch() {
    const q = query.trim();
    // trigram 分词要求 ≥3 字符，否则退化为全表扫描（方案 §7.3）
    if (q.length < 3) {
      results = [];
      searched = false;
      return;
    }
    if (!vault.root) return;
    searching = true;
    try {
      const v = await upsertVault(vault.name, vault.root);
      if (!v) return;
      results = await searchDocs(v.id, q);
      searched = true;
    } catch (e) {
      vault.showToast(errorMessage(e));
    } finally {
      searching = false;
    }
  }

  /**
   * snippet() 用哨兵串 / 包裹命中词（方案 §7.3）。
   * 必须先转义正文再替换为 <mark>，否则文档里的 <script> 会经搜索结果注入。
   */
  function excerptHtml(excerpt: string): string {
    const escaped = excerpt
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;')
      .replace(/"/g, '&quot;');
    return escaped.replaceAll('', '<mark>').replaceAll('', '</mark>');
  }

  function baseName(p: string): string {
    return entryName(p);
  }

  async function openHit(hit: SearchHit) {
    try {
      await nav.open(hit.entry, hit.title ?? baseName(hit.entry));
      onClose();
    } catch (e) {
      vault.showToast(errorMessage(e));
    }
  }
</script>

<div class="drawer-header">
  <span>搜索</span>
  <button class="icon-btn" title="关闭" onclick={onClose}><Icon name="close" size={14} /></button>
</div>

<div class="search-input-row">
  <input
    class="search-input"
    type="search"
    placeholder="跨文件搜索（至少 3 个字）…"
    bind:value={query}
    oninput={onInput}
  />
</div>

{#if vault.indexing && vault.indexProgress}
  <div class="index-banner">
    正在建立索引… {vault.indexProgress.done}/{vault.indexProgress.total}
  </div>
{/if}

<div class="drawer-body">
  {#if !vault.root}
    <div class="search-hint">请先选择文档库</div>
  {:else if query.trim().length > 0 && query.trim().length < 3}
    <div class="search-hint">请输入至少 3 个字（中文检索基于 trigram 分词）</div>
  {:else if searching}
    <div class="search-hint">搜索中…</div>
  {:else if searched && results.length === 0}
    <div class="search-hint">
      没有匹配的结果。
      {#if !vault.indexing}
        <button class="secondary-btn" style="margin-top:8px" onclick={() => vault.startIndexing()}>
          重新建立索引
        </button>
      {/if}
    </div>
  {:else}
    {#each results as hit (hit.doc_id)}
      <button class="search-result" onclick={() => openHit(hit)}>
        <div class="r-title">{hit.title ?? baseName(hit.entry)}</div>
        <div class="r-path">{displayPath(hit.entry, vault.root)}</div>
        <!-- excerpt 已先做 HTML 转义再插入 <mark>，见 excerptHtml -->
        <div class="r-excerpt">{@html excerptHtml(hit.excerpt)}</div>
      </button>
    {/each}
  {/if}
</div>
