<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs } from '../lib/stores/prefs.svelte';
  import { exportRequest } from '../lib/stores/export.svelte';
  import { api } from '../lib/api';
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

  // 移动端（Android）window.print() 是空实现，导出 PDF 点了没反应，隐藏该入口
  let isMobile = $state(false);
  onMount(() => {
    api.isMobile().then((v) => (isMobile = v)).catch(() => (isMobile = false));
  });

  /** 导出 PDF：关闭抽屉后请求 ArticleView 渲染全文，再由 Rust 端走 WebView2 PrintToPdf（仅桌面端） */
  function exportPdf() {
    onClose();
    exportRequest.request();
  }
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

  {#if !isMobile}
    <div class="setting-group">
      <div class="setting-label">导出</div>
      <button class="primary-btn" onclick={exportPdf}>导出为 PDF</button>
      <div class="setting-hint">
        技术文档排版：Times 衬线正文、浅灰代码块、蓝色引用卡片、全边框表格。<br />
        点击后将弹出保存对话框，直接生成排版好的 PDF。
      </div>
    </div>
  {/if}
</div>
