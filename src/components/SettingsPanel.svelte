<script lang="ts">
  import { prefs } from '../lib/stores/prefs.svelte';
  import Icon from './Icons.svelte';

  interface Props {
    onClose: () => void;
  }
  let { onClose }: Props = $props();

  const themes = [
    { id: 'github-light', name: 'GitHub 浅色' },
    { id: 'github-dark', name: 'GitHub 深色' },
    { id: 'sepia', name: '护眼米黄' },
  ];

  const widths = [
    { id: 'compact', name: '紧凑' },
    { id: 'comfortable', name: '舒适' },
    { id: 'wide', name: '宽松' },
  ];
</script>

<div class="drawer-header">
  <span>设置</span>
  <button class="icon-btn" title="关闭" onclick={onClose}><Icon name="close" size={14} /></button>
</div>
<div class="drawer-body">
  <div class="setting-group">
    <div class="setting-label">主题</div>
    <div class="theme-options">
      {#each themes as t (t.id)}
        <button
          class="theme-swatch"
          data-preview={t.id}
          class:active={prefs.theme === t.id}
          onclick={() => (prefs.theme = t.id)}
        >
          <span class="swatch-dots"><i></i><i></i><i></i></span>
          {t.name}
        </button>
      {/each}
    </div>
  </div>

  <div class="setting-group">
    <div class="setting-label">正文宽度</div>
    <div class="theme-options">
      {#each widths as w (w.id)}
        <button
          class="theme-swatch"
          class:active={prefs.measure === w.id}
          onclick={() => (prefs.measure = w.id)}
        >{w.name}</button>
      {/each}
    </div>
  </div>

  <div class="setting-group">
    <div class="setting-label">字号</div>
    <div class="slider-row">
      <input
        type="range"
        min="13"
        max="24"
        step="1"
        value={prefs.fontSize}
        oninput={(e) => (prefs.fontSize = Number(e.currentTarget.value))}
      />
      <span class="slider-value">{prefs.fontSize}px</span>
    </div>
  </div>

  <div class="setting-group">
    <div class="setting-label">行距</div>
    <div class="slider-row">
      <input
        type="range"
        min="1.4"
        max="2.2"
        step="0.05"
        value={prefs.lineHeight}
        oninput={(e) => (prefs.lineHeight = Number(e.currentTarget.value))}
      />
      <span class="slider-value">{prefs.lineHeight.toFixed(2)}</span>
    </div>
  </div>
</div>
