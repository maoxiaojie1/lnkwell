<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs } from '../lib/stores/prefs.svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { lastVault, recentDocs, type RecentDoc } from '../lib/db';
  import { api } from '../lib/api';
  import Icon from './Icons.svelte';

  let isMobile = $state(false);
  let recents = $state<RecentDoc[]>([]);

  onMount(async () => {
    isMobile = await api.isMobile().catch(() => false);
    try {
      recents = await recentDocs(5);
    } catch {
      recents = [];
    }
  });

  async function pickVault() {
    const ok = await vault.pick();
    if (!ok) return;
    const last = await lastVault();
    if (last) await vault.setRoot(last.root_uri);
  }

  function openRecent(r: RecentDoc) {
    void nav.open(r.rel_path, r.title);
  }
</script>

<div class="welcome">
  <!-- iOS App Icon 风格的 logo（带渐变） -->
  <div class="brand" aria-hidden="true">
    <div class="brand-mark"></div>
  </div>

  <div class="title">Lnkwell</div>
  <p class="tagline">离线优先的 Markdown 阅读器<br />把本地文档渲染得和 GitHub 一样好看</p>

  <button class="primary-btn" onclick={pickVault}>
    <Icon name="folder" size={18} />
    选择文档库
  </button>

  {#if recents.length > 0}
    <div class="recent-heading">最近阅读</div>
    <div class="recent-list">
      {#each recents as r (r.rel_path)}
        <button class="recent-item" onclick={() => openRecent(r)}>
          <div class="title-row">{r.title}</div>
          <div class="path">{r.rel_path}</div>
        </button>
      {/each}
    </div>
  {/if}
</div>

<style>
  .welcome {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    /* iOS 风格：内容垂直偏上，避免与底部 Tab Bar 抢眼 */
    justify-content: center;
    padding: 32px 24px calc(40px + var(--safe-bottom, 0px));
    text-align: center;
    gap: 0;
    /* 背景用主题色而非纯白（github-light 的 --md-bg 是 #fbfcfd） */
    background: var(--md-bg);
  }

  .brand {
    margin-bottom: 28px;
  }
  /* iOS App Icon：纯色 accent + 顶部弧形高光 + 底部微暗，模拟真实立体感 */
  .brand-mark {
    width: 80px;
    height: 80px;
    border-radius: 20px; /* iOS 连续圆角 */
    background: var(--ui-accent);
    /* 真实 App Icon 的立体感：外层投影 + 内部顶部高光 */
    box-shadow:
      0 12px 28px color-mix(in srgb, var(--ui-accent) 32%, transparent),
      0 2px 4px color-mix(in srgb, var(--ui-accent) 18%, transparent),
      inset 0 1.5px 1px rgba(255, 255, 255, 0.35),
      inset 0 -1.5px 1px rgba(0, 0, 0, 0.15);
    position: relative;
  }
  /* 顶部弧形高光（从左上 45° 的柔和光晕，iOS App Icon 标志性的光感） */
  .brand-mark::before {
    content: "";
    position: absolute;
    left: -20%;
    top: -20%;
    width: 140%;
    height: 140%;
    background: radial-gradient(
      circle at 30% 30%,
      rgba(255, 255, 255, 0.25) 0%,
      rgba(255, 255, 255, 0) 50%
    );
    border-radius: 20px;
    pointer-events: none;
  }

  .welcome :global(.title),
  .title {
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.025em;
    color: var(--ui-fg);
    margin: 0 0 12px;
    line-height: 1.1;
  }

  .tagline {
    font-size: 16px;
    line-height: 1.55;
    color: var(--ui-fg-muted);
    margin: 0 0 36px;
    max-width: 18em;
    font-weight: 400;
    letter-spacing: 0.005em;
  }

  /* iOS Blue 按钮：用系统蓝 #007aff（与主题 accent 区分，更"原生"） */
  .primary-btn {
    min-width: 220px;
    padding: 15px 32px;
    border-radius: 14px;
    font-size: 17px;
    font-weight: 600;
    letter-spacing: 0.01em;
    background: var(--ios-blue);
    /* iOS Blue 的彩色投影（更深、更柔） */
    box-shadow:
      0 8px 24px color-mix(in srgb, var(--ios-blue) 40%, transparent),
      0 2px 6px color-mix(in srgb, var(--ios-blue) 20%, transparent);
    transition: transform 0.12s ease, box-shadow 0.2s ease, filter 0.15s ease;
  }
  .primary-btn:hover {
    filter: brightness(1.06);
    box-shadow:
      0 12px 32px color-mix(in srgb, var(--ios-blue) 48%, transparent),
      0 4px 10px color-mix(in srgb, var(--ios-blue) 25%, transparent);
  }
  .primary-btn:active {
    transform: translateY(1px) scale(0.99);
    box-shadow:
      0 4px 16px color-mix(in srgb, var(--ios-blue) 32%, transparent);
  }

  .recent-heading {
    font-size: 13px;
    font-weight: 600;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    color: var(--ui-fg-muted);
    margin: 44px 0 14px;
    align-self: stretch;
    text-align: left;
    max-width: 480px;
  }

  .recent-list {
    width: 100%;
    max-width: 480px;
    text-align: left;
  }
  /* 最近阅读项：更柔和的卡片 */
  .recent-item {
    display: block;
    width: 100%;
    text-align: left;
    padding: 14px 16px;
    margin-bottom: 8px;
    border: none;
    border-radius: 12px;
    background: var(--ui-bg);
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.04);
    cursor: pointer;
    color: var(--ui-fg);
    transition: transform 0.15s ease, box-shadow 0.2s ease;
  }
  .recent-item:hover {
    transform: translateY(-1px);
    box-shadow: 0 4px 16px rgba(0, 0, 0, 0.08);
  }
  .recent-item:active {
    transform: scale(0.98);
  }
  .recent-item .title-row {
    font-size: 16px;
    font-weight: 600;
    letter-spacing: -0.005em;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .recent-item .path {
    font-size: 13px;
    color: var(--ui-fg-muted);
    margin-top: 3px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  /* 窄屏：更紧凑 */
  @media (max-width: 768px), (pointer: coarse) {
    .brand-mark { width: 72px; height: 72px; border-radius: 18px; }
    .title { font-size: 30px; }
    .tagline { font-size: 15px; }
    .primary-btn {
      min-width: 0;
      width: min(100%, 320px);
    }
    .recent-list { max-width: 100%; }
  }
</style>