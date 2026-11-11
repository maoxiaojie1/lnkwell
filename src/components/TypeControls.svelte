<script lang="ts">
  import { prefs, Prefs } from '../lib/stores/prefs.svelte';

  let open = $state(false);
  let rootEl: HTMLElement | undefined = $state();

  const widths = [
    { id: 'compact', name: '窄' },
    { id: 'comfortable', name: '适中' },
    { id: 'wide', name: '宽' },
  ];

  function stepSize(delta: number) {
    const base = prefs.effectiveFontSize();
    prefs.fontSize = Math.min(24, Math.max(13, Math.round(base + delta)));
  }

  // 点空白处收起
  $effect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      if (rootEl && !rootEl.contains(e.target as Node)) open = false;
    };
    document.addEventListener('mousedown', onDown);
    return () => document.removeEventListener('mousedown', onDown);
  });
</script>

<div class="type-controls" bind:this={rootEl}>
  <button
    class="aa-btn"
    class:active={open}
    title="阅读设置（缩放：Ctrl + 滚轮 / Ctrl ± / Ctrl 0）"
    onclick={() => (open = !open)}
  >Aa</button>

  {#if open}
    <div class="aa-panel" role="dialog" aria-label="阅读设置">
      <div class="aa-row">
        <span class="aa-label">缩放</span>
        <div class="aa-stepper">
          <button
            title="缩小（Ctrl -）"
            disabled={prefs.zoom <= Prefs.ZOOM_MIN}
            onclick={() => prefs.zoomOut()}
          >−</button>
          <button
            class="aa-value aa-num"
            title="复位 100%（Ctrl 0）"
            onclick={() => prefs.resetZoom()}
          >{Math.round(prefs.zoom * 100)}%</button>
          <button
            title="放大（Ctrl +）"
            disabled={prefs.zoom >= Prefs.ZOOM_MAX}
            onclick={() => prefs.zoomIn()}
          >+</button>
        </div>
      </div>

      <div class="aa-row">
        <span class="aa-label">字号</span>
        <div class="aa-stepper">
          <button
            title="减小字号"
            disabled={prefs.effectiveFontSize() <= 13}
            onclick={() => stepSize(-1)}
          >A−</button>
          <span class="aa-value">{prefs.effectiveFontSize()}</span>
          <button
            title="增大字号"
            disabled={prefs.effectiveFontSize() >= 24}
            onclick={() => stepSize(1)}
          >A+</button>
        </div>
      </div>

      <div class="aa-row">
        <span class="aa-label">宽度</span>
        <div class="aa-seg">
          {#each widths as w (w.id)}
            <button class:active={prefs.measure === w.id} onclick={() => (prefs.measure = w.id)}>
              {w.name}
            </button>
          {/each}
        </div>
      </div>

      <div class="aa-row">
        <span class="aa-label">行距</span>
        <input
          class="aa-range"
          type="range"
          min="1.4"
          max="2.2"
          step="0.05"
          value={prefs.effectiveLineHeight()}
          oninput={(e) => (prefs.lineHeight = Number(e.currentTarget.value))}
          aria-label="行距"
        />
        <span class="aa-value">{prefs.effectiveLineHeight().toFixed(2)}</span>
      </div>
    </div>
  {/if}
</div>
