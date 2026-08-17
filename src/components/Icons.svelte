<script lang="ts">
  /**
   * 统一线性图标集（stroke 风格，lucide 风格）。
   * 用 currentColor 描边，随文字颜色自动适配主题。
   */
  export type IconName =
    | 'menu' | 'back' | 'toc' | 'search' | 'star' | 'settings'
    | 'refresh' | 'folder' | 'close' | 'file' | 'chevron' | 'open' | 'image'
    | 'find' | 'up' | 'down' | 'library';

  interface Props {
    name: IconName;
    size?: number;
  }
  let { name, size = 16 }: Props = $props();
</script>

<svg
  class="icon"
  width={size}
  height={size}
  viewBox="0 0 24 24"
  fill="none"
  stroke="currentColor"
  stroke-width="1.8"
  stroke-linecap="round"
  stroke-linejoin="round"
  aria-hidden="true"
>
  {#if name === 'menu'}
    <path d="M4 6h16" /><path d="M4 12h16" /><path d="M4 18h16" />
  {:else if name === 'back'}
    <path d="M19 12H5" /><path d="m11 18-6-6 6-6" />
  {:else if name === 'toc'}
    <path d="M4 6h16" /><path d="M4 12h10" /><path d="M4 18h14" />
  {:else if name === 'search'}
    <circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" />
  {:else if name === 'star'}
    <path d="m12 2 3.09 6.26L22 9.27l-5 4.87 1.18 6.88L12 17.77l-6.18 3.25L7 14.14 2 9.27l6.91-1.01z" />
  {:else if name === 'settings'}
    <circle cx="12" cy="12" r="3" />
    <path d="M19.4 15a1.65 1.65 0 0 0 .33 1.82l.06.06a2 2 0 1 1-2.83 2.83l-.06-.06a1.65 1.65 0 0 0-1.82-.33 1.65 1.65 0 0 0-1 1.51V21a2 2 0 1 1-4 0v-.09A1.65 1.65 0 0 0 9 19.4a1.65 1.65 0 0 0-1.82.33l-.06.06a2 2 0 1 1-2.83-2.83l.06-.06a1.65 1.65 0 0 0 .33-1.82 1.65 1.65 0 0 0-1.51-1H3a2 2 0 1 1 0-4h.09A1.65 1.65 0 0 0 4.6 9a1.65 1.65 0 0 0-.33-1.82l-.06-.06a2 2 0 1 1 2.83-2.83l.06.06a1.65 1.65 0 0 0 1.82.33H9a1.65 1.65 0 0 0 1-1.51V3a2 2 0 1 1 4 0v.09a1.65 1.65 0 0 0 1 1.51 1.65 1.65 0 0 0 1.82-.33l.06-.06a2 2 0 1 1 2.83 2.83l-.06.06a1.65 1.65 0 0 0-.33 1.82V9a1.65 1.65 0 0 0 1.51 1H21a2 2 0 1 1 0 4h-.09a1.65 1.65 0 0 0-1.51 1z" />
  {:else if name === 'refresh'}
    <path d="M21 12a9 9 0 1 1-2.64-6.36" /><path d="M21 3v6h-6" />
  {:else if name === 'folder'}
    <path d="M20 20a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.69-.9L9.6 3.9A2 2 0 0 0 7.93 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2Z" />
  {:else if name === 'close'}
    <path d="M18 6 6 18" /><path d="m6 6 12 12" />
  {:else if name === 'file'}
    <path d="M15 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7Z" />
    <path d="M14 2v4a2 2 0 0 0 2 2h4" />
  {:else if name === 'chevron'}
    <path d="m9 18 6-6-6-6" />
  {:else if name === 'open'}
    <path d="M15 3h6v6" /><path d="M10 14 21 3" />
    <path d="M18 13v6a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2h6" />
  {:else if name === 'image'}
    <rect x="3" y="3" width="18" height="18" rx="2" ry="2" />
    <circle cx="8.5" cy="8.5" r="1.5" />
    <path d="m21 15-5-5L5 21" />
  {:else if name === 'find'}
    <circle cx="11" cy="11" r="7" /><path d="m21 21-4.3-4.3" />
    <path d="M11 7v6" /><path d="m8.5 11 2.5 2 2.5-2" />
  {:else if name === 'up'}
    <path d="m18 15-6-6-6 6" />
  {:else if name === 'down'}
    <path d="m6 9 6 6 6-6" />
  {:else if name === 'library'}
    <path d="M3 4h4v16H3z" />
    <path d="M10 5.5 13 4l5 2.5v15L13 19l-3-1.5z" />
    <path d="m13 4 5 2.5v15L13 19" />
  {/if}
</svg>

<style>
  .icon {
    display: block;
    flex-shrink: 0;
  }
</style>
