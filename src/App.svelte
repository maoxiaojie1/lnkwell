<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs } from './lib/stores/prefs.svelte';
  import { vault } from './lib/stores/vault.svelte';
  import { nav } from './lib/stores/nav.svelte';
  import { find } from './lib/stores/find.svelte';
  import { lastVault } from './lib/db';
  import { api } from './lib/api';
  import FileTree from './components/FileTree.svelte';
  import ArticleView from './components/ArticleView.svelte';
  import TocDrawer from './components/TocDrawer.svelte';
  import SearchPanel from './components/SearchPanel.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import BookmarksPanel from './components/BookmarksPanel.svelte';
  import ImageViewer from './components/ImageViewer.svelte';
  import Welcome from './components/Welcome.svelte';
  import Icon from './components/Icons.svelte';

  type DrawerKind = 'none' | 'toc' | 'search' | 'bookmarks' | 'settings';
  let drawer = $state<DrawerKind>('none');
  let viewerSrc = $state<string | null>(null);

  function closeViewer() {
    viewerSrc = null;
  }

  /**
   * 窄屏/移动端判定。Tauri Android WebView 的 innerWidth 可能报布局视口宽度（>物理 CSS 宽），
   * 不可靠。触屏 + 窄物理屏才是移动形态；用 matchMedia(pointer) 与最小视口综合判定。
   */
  function narrowNow(): boolean {
    if (typeof window === 'undefined') return false;
    // 触屏设备优先按移动端处理（侧栏抽屉化）
    const coarse = window.matchMedia('(pointer: coarse)').matches;
    const w = Math.min(
      window.innerWidth || Infinity,
      window.visualViewport?.width ?? Infinity,
      window.screen?.width ?? Infinity
    );
    return w <= 768 || coarse;
  }

  let sidebarOpen = $state(true);

  const isNarrow = () => narrowNow();

  // 窄屏下选完库自动【展开】侧栏，让用户看到刚加载的目录树
  async function pickVault() {
    const ok = await vault.pick();
    if (ok && narrowNow()) sidebarOpen = true;
  }

  onMount(() => {
    // 挂载后再判定一次窄屏（此时视口已稳定），窄屏默认收起侧栏
    sidebarOpen = !narrowNow();
    // 异步初始化放 IIFE 里，onMount 本体保持同步返回 cleanup
    let unlistenShare: (() => void) | undefined;
    void (async () => {
      await prefs.hydrate();
      // 恢复上次打开的文档库
      const last = await lastVault();
      if (last) await vault.setRoot(last.root_uri);
      // 监听系统分享（方案 §7.4）：deep-link 插件通过 open-url 事件传递 content:// URI
      unlistenShare = await nav.listenForShare();

      // 移动端状态栏兜底：仅 Android/iOS 需要，桌面端保持 --safe-top: 0px。
      // env(safe-area-inset-top) 在 Tauri Android 不稳定（时 49px 时 0px）。
      // 实测：返回 0 时工具栏顶到状态栏下被系统手势拦截，点不了。
      // 方案：读真实值，为 0 时用固定 24px（Android 状态栏标准高度）兜底，设为 CSS 变量。
      if (await api.isMobile()) {
        const probe = document.createElement('div');
        probe.style.paddingTop = 'env(safe-area-inset-top, 0px)';
        document.body.appendChild(probe);
        const satRaw = getComputedStyle(probe).paddingTop;
        probe.remove();
        const satPx = parseFloat(satRaw);
        const safeTop = satPx > 0 ? satRaw : '24px';
        document.documentElement.style.setProperty('--safe-top', safeTop);
      }
    })();
    return () => unlistenShare?.();
  });

  // 返回键 / 浏览器后退：查找栏打开→关查找；抽屉打开→关抽屉；有导航历史→回上一篇（方案 §7.1）
  function onKeydown(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      if (find.open) { find.close(); return; }
      handleBack();
      return;
    }
    if (e.key === 'BrowserBack' || (e.altKey && e.key === 'ArrowLeft')) {
      handleBack();
    }
  }

  function handleBack(): boolean {
    if (viewerSrc) { closeViewer(); return true; }
    if (drawer !== 'none') { drawer = 'none'; return true; }
    return nav.back();
  }

  function toggleDrawer(kind: DrawerKind) {
    drawer = drawer === kind ? 'none' : kind;
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="layout">
  <aside class="sidebar" class:collapsed={!sidebarOpen}>
    <div class="sidebar-header">
      <div class="sidebar-brand">
        <span class="brand-mark"></span>
        <span class="sidebar-title" title={vault.root ?? ''}>
          {vault.name || 'Lnkwell'}
        </span>
      </div>
      <div class="sidebar-actions">
        <button class="icon-btn" title="刷新目录" onclick={() => vault.refresh()}>
          <Icon name="refresh" size={15} />
        </button>
        <button class="icon-btn" title="更换文档库" onclick={pickVault}>
          <Icon name="folder" size={15} />
        </button>
      </div>
    </div>
    <FileTree
      onOpenFile={() => { if (isNarrow()) sidebarOpen = false; }}
      onOpenImage={(src) => (viewerSrc = src)}
    />
  </aside>

  <!-- 窄屏侧栏展开时的遮罩：点击收起 -->
  {#if sidebarOpen && isNarrow()}
    <div
      class="sidebar-backdrop"
      role="presentation"
      onclick={() => (sidebarOpen = false)}
    ></div>
  {/if}

  <main class="main-area">
    <div class="toolbar">
      <button
        class="icon-btn"
        title="侧栏"
        onclick={() => (sidebarOpen = !sidebarOpen)}
      ><Icon name="menu" /></button>
      <button class="icon-btn" title="返回" disabled={!nav.canGoBack} onclick={() => nav.back()}>
        <Icon name="back" />
      </button>
      {#if nav.current}
        <span class="doc-title">{nav.current.result.title ?? nav.current.name}</span>
      {/if}
      <span class="spacer"></span>
      <div class="toolbar-group">
        <button
          class="icon-btn"
          class:active={drawer === 'toc'}
          title="目录"
          onclick={() => toggleDrawer('toc')}
        ><Icon name="toc" /></button>
        <button
          class="icon-btn"
          class:active={drawer === 'search'}
          title="搜索"
          onclick={() => toggleDrawer('search')}
        ><Icon name="search" /></button>
        <button
          class="icon-btn"
          class:active={find.open}
          title="文档内查找 (Ctrl+F)"
          onclick={() => find.toggle()}
        ><Icon name="find" /></button>
        <button
          class="icon-btn"
          class:active={drawer === 'bookmarks'}
          title="书签"
          onclick={() => toggleDrawer('bookmarks')}
        ><Icon name="star" /></button>
        <button
          class="icon-btn"
          class:active={drawer === 'settings'}
          title="设置"
          onclick={() => toggleDrawer('settings')}
        ><Icon name="settings" /></button>
      </div>
    </div>

    {#if vault.indexing && vault.indexProgress}
      <div class="index-banner">
        正在建立搜索索引… {vault.indexProgress.done}/{vault.indexProgress.total}
      </div>
    {/if}

    {#if nav.current}
      <ArticleView doc={nav.current} onOpenImage={(src) => (viewerSrc = src)} />
    {:else}
      <Welcome />
    {/if}
  </main>
</div>

{#if drawer !== 'none'}
  <div class="drawer-backdrop" role="presentation" onclick={() => (drawer = 'none')}></div>
  <div class="drawer">
    {#if drawer === 'toc'}
      <TocDrawer onClose={() => (drawer = 'none')} />
    {:else if drawer === 'search'}
      <SearchPanel onClose={() => (drawer = 'none')} />
    {:else if drawer === 'bookmarks'}
      <BookmarksPanel onClose={() => (drawer = 'none')} />
    {:else}
      <SettingsPanel onClose={() => (drawer = 'none')} />
    {/if}
  </div>
{/if}

{#if viewerSrc}
  <ImageViewer src={viewerSrc} onClose={closeViewer} />
{/if}

{#if vault.toast}
  <div class="toast">{vault.toast}</div>
{/if}
