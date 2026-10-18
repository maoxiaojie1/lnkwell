<script lang="ts">
  import { onMount } from 'svelte';
  import { api, errorMessage } from '../lib/api';
  import { vault } from '../lib/stores/vault.svelte';
  import type { OpenDoc } from '../lib/stores/nav.svelte';
  import Icon from './Icons.svelte';

  /**
   * 只读源码视图。
   *
   * 为什么阅读器要给这个口子：技术文档渲染出错时（公式、表格、嵌套列表），
   * 用户需要一眼确认「是文档写错了还是渲染器的问题」；另外复制原文语法
   * （$E=mc^2$、**加粗**）在渲染视图里做不到。
   *
   * 边界很明确：只读 —— 没有输入框、没有保存，顶部直接标「只读」，
   * 避免让人期待一个编辑器。
   */
  interface Props {
    doc: OpenDoc;
    onExit: () => void;
  }
  let { doc, onExit }: Props = $props();

  let text = $state<string | null>(null);
  let error = $state<string | null>(null);
  let copied = $state(false);

  onMount(async () => {
    try {
      text = await api.readRawDoc(doc.entry);
    } catch (e) {
      error = errorMessage(e);
    }
  });

  // 末尾的换行不留一个空行号
  let lines = $derived((text ?? '').replace(/\n$/, '').split('\n'));
  let chars = $derived((text ?? '').length);

  async function copyAll() {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      vault.showToast('已复制全文源码');
      setTimeout(() => (copied = false), 1600);
    } catch (e) {
      vault.showError(e);
    }
  }
</script>

<div class="source-view">
  <div class="source-bar">
    <span class="source-badge">只读</span>
    <span class="source-meta">
      {#if text !== null}{lines.length} 行 · {chars.toLocaleString()} 字符{/if}
    </span>
    <span class="spacer"></span>
    <button class="source-act" onclick={copyAll} disabled={!text}>
      <Icon name="file" size={13} />
      {copied ? '已复制' : '复制全文'}
    </button>
    <button class="source-act" onclick={onExit}>
      <Icon name="close" size={13} />
      返回阅读
    </button>
  </div>

  <div class="source-scroll">
    {#if error}
      <div class="source-hint">读取失败：{error}</div>
    {:else if text === null}
      <div class="source-hint">读取中…</div>
    {:else}
      <div class="source-body">
        {#each lines as line, i (i)}
          <!-- 行号不参与选中，复制出去是干净源码 -->
          <div class="src-line"><span class="src-no">{i + 1}</span><span class="src-txt">{line || ' '}</span></div>
        {/each}
      </div>
    {/if}
  </div>
</div>

<style>
  .source-view {
    flex: 1;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--md-bg);
  }

  .source-bar {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 8px 16px;
    border-bottom: 1px solid var(--ui-border-soft);
    background: var(--ui-bg-alt);
    font-size: 12px;
    color: var(--ui-fg-muted);
  }
  .source-bar .spacer { flex: 1; }

  .source-badge {
    padding: 2px 7px;
    border-radius: 999px;
    border: 1px solid var(--ui-border);
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.04em;
    color: var(--ui-fg-muted);
  }
  .source-meta { font-variant-numeric: tabular-nums; }

  .source-act {
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
  .source-act:hover:not(:disabled) {
    background: var(--ui-bg-hover);
    border-color: color-mix(in srgb, var(--ui-accent) 40%, var(--ui-border));
    color: var(--ui-fg);
  }
  .source-act:disabled { opacity: 0.5; cursor: default; }

  .source-scroll {
    flex: 1;
    min-height: 0;
    overflow: auto;
  }
  .source-hint {
    padding: 24px;
    font-size: 13px;
    color: var(--ui-fg-muted);
  }

  .source-body {
    padding: 14px 0 40px;
    font-family: var(--md-font-mono);
    font-size: 12.5px;
    line-height: 1.75;
    color: var(--ui-fg-2);
    tab-size: 4;
    min-width: max-content;
  }

  .src-line {
    display: flex;
    align-items: flex-start;
    padding-right: 24px;
  }
  .src-line:hover { background: color-mix(in srgb, var(--ui-fg) 4%, transparent); }

  .src-no {
    flex-shrink: 0;
    width: 52px;
    padding-right: 14px;
    text-align: right;
    color: var(--ui-fg-muted);
    opacity: 0.55;
    user-select: none;
    font-variant-numeric: tabular-nums;
  }
  .src-txt {
    white-space: pre;
    flex: 1;
  }

  /* 窄屏：行数字符统计和信息按钮挤在一行会换行，收掉统计 */
  @media (max-width: 768px), (pointer: coarse) {
    .source-bar { gap: 8px; padding: 8px 12px; }
    .source-meta { display: none; }
    .source-act { padding: 5px 9px; }
    .src-no { width: 40px; padding-right: 10px; }
    .source-body { font-size: 13px; }
  }
</style>
