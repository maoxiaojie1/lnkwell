<script lang="ts">
  import { api } from '../lib/api';
  import type { FileNode } from '../lib/api';
  import { vault } from '../lib/stores/vault.svelte';
  import TreeNodeSelf from './TreeNode.svelte';
  import Icon from './Icons.svelte';

  interface Props {
    node: FileNode;
    depth: number;
    activeEntry: string;
    onOpen: (node: FileNode) => void;
  }
  let { node, depth, activeEntry, onOpen }: Props = $props();

  let expanded = $state(false);
  let children = $state<FileNode[] | null>(null);
  let loadingChildren = $state(false);

  function isImage(name: string): boolean {
    return /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/i.test(name);
  }

  async function toggle() {
    if (!node.is_dir) {
      onOpen(node);
      return;
    }
    expanded = !expanded;
    if (expanded && children === null) {
      loadingChildren = true;
      try {
        children = await api.listDir(node.entry);
      } catch (e) {
        vault.showError(e);
        children = [];
      } finally {
        loadingChildren = false;
      }
    }
  }
</script>

<button
  class="tree-row"
  class:active={!node.is_dir && node.entry === activeEntry}
  style:padding-left="{10 + depth * 4}px"
  title={node.name}
  onclick={toggle}
>
  <span class="tree-icon" class:expanded={node.is_dir && expanded}>
    {#if node.is_dir}
      <Icon name="chevron" size={13} />
    {:else if isImage(node.name)}
      <Icon name="image" size={14} />
    {:else}
      <Icon name="file" size={14} />
    {/if}
  </span>
  <span style="overflow:hidden;text-overflow:ellipsis">{node.name}</span>
</button>

{#if node.is_dir && expanded}
  {#if loadingChildren}
    <div class="search-hint" style:padding-left="{20 + depth * 4}px">…</div>
  {:else if children}
    <div class="tree-children">
      {#each children as child (child.entry)}
        <TreeNodeSelf node={child} depth={depth + 1} {activeEntry} {onOpen} />
      {/each}
    </div>
  {/if}
{/if}
