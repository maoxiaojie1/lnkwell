<script lang="ts">
  import { nav } from '../lib/stores/nav.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import type { TocItem } from '../lib/api';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  let toc = $derived(nav.current?.result.toc ?? []);

  function jump(item: TocItem) {
    // 等 drawer 关闭后再滚动，避免布局抖动
    onClose();
    // 委托给 ArticleView：携带目标 chunk 序号，一步渲染到位再滚动
    tocJump.request(item.id, item.chunk);
  }
</script>

<div class="drawer-header">
  <span>目录</span>
  <button class="icon-btn" title="关闭" onclick={onClose}><Icon name="close" size={14} /></button>
</div>
<div class="drawer-body">
  {#if !nav.current}
    <div class="search-hint">尚未打开文档</div>
  {:else if toc.length === 0}
    <div class="search-hint">本文档没有标题</div>
  {:else}
    {#each toc as item (item.id)}
      <button
        class="toc-item lvl-{item.level}"
        style:padding-left="{16 + (item.level - 1) * 16}px"
        onclick={() => jump(item)}
      >{item.text}</button>
    {/each}
  {/if}
</div>
