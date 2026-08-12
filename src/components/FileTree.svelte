<script lang="ts">
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { api, errorMessage } from '../lib/api';
  import type { FileNode } from '../lib/api';
  import TreeNode from './TreeNode.svelte';

  interface Props {
    onOpenFile?: () => void;
    /** 点击图片文件：读出 data URL 后交给 ImageViewer */
    onOpenImage?: (src: string) => void;
  }
  let { onOpenFile, onOpenImage }: Props = $props();

  function isImage(name: string): boolean {
    return /\.(png|jpe?g|gif|webp|bmp|svg|ico)$/i.test(name);
  }

  async function openFile(node: FileNode) {
    try {
      if (isImage(node.name)) {
        onOpenImage?.(await api.readImage(node.entry));
        onOpenFile?.();
        return;
      }
      await nav.open(node.entry, node.name, { replace: true });
      onOpenFile?.();
    } catch (e) {
      vault.showToast(errorMessage(e));
    }
  }
</script>

{#if !vault.root}
  <div class="search-hint">尚未选择文档库</div>
{:else if vault.loading}
  <div class="search-hint">读取目录中…</div>
{:else if vault.topNodes.length === 0}
  <div class="search-hint">此文件夹中没有 Markdown 或图片文件</div>
{:else}
  <nav class="file-tree">
    {#each vault.topNodes as node (node.entry)}
      <TreeNode {node} depth={0} onOpen={openFile} activeEntry={nav.current?.entry ?? ''} />
    {/each}
  </nav>
{/if}
