<script lang="ts">
  import { nav } from '../lib/stores/nav.svelte';
  import { tocJump } from '../lib/stores/tocJump.svelte';
  import type { TocItem } from '../lib/api';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
    /** 侧栏模式下跳转不应关闭面板（目录与正文同层时它本来就是常驻的） */
    closeOnJump?: boolean;
  }
  let { onClose, closeOnJump = true }: Props = $props();

  let toc = $derived(nav.current?.result.toc ?? []);

  function jump(item: TocItem) {
    // 抽屉模式：先关闭再滚动，避免布局抖动；侧栏模式保持展开
    if (closeOnJump) onClose();
    // 委托给 ArticleView：携带目标 chunk 序号，一步渲染到位再滚动
    tocJump.request(item.id, item.chunk);
  }
</script>

<div class="drawer-header">
  <span>目录</span>
  <button class="icon-btn" title={closeOnJump ? '关闭' : '收起目录'} onclick={onClose}>
    <Icon name="close" size={14} />
  </button>
</div>
<div class="drawer-body">
  {#if !nav.current}
    <div class="search-hint">尚未打开文档</div>
  {:else if toc.length === 0}
    <div class="search-hint">本文档没有标题</div>
  {:else}
    {#each toc as item (item.id)}
      <!-- 层级缩进交给 CSS（--toc-depth），不在组件里写死 padding -->
      <button
        class="toc-item lvl-{item.level}"
        style:--toc-depth={Math.min(item.level - 1, 4)}
        onclick={() => jump(item)}
      >{item.text}</button>
    {/each}
  {/if}
</div>
