<script lang="ts">
  import { onMount } from 'svelte';
  import { prefs } from './lib/stores/prefs.svelte';
  import { vault } from './lib/stores/vault.svelte';
  import { nav } from './lib/stores/nav.svelte';
  import { find } from './lib/stores/find.svelte';
  import { lastVault, dbStatus } from './lib/db';
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

  /**
   * 宽屏右侧目录栏：与正文同层（flex 兄弟节点），可展开/收缩。
   * 窄屏仍走全屏抽屉（空间不够，侧栏会把正文压得太窄）。
   */
  let tocOpen = $state(false);
  /** 用户手动切换过目录栏后，不再自动展开（尊重用户选择） */
  let tocTouched = false;
  /** 窗口宽度达到该值才默认展开目录栏 */
  const TOC_AUTO_OPEN_MIN_WIDTH = 1100;

  function toggleToc() {
    tocOpen = !tocOpen;
    tocTouched = true;
  }
  function closeToc() {
    tocOpen = false;
    tocTouched = true;
  }

  // 首次打开文档（或从窄屏切到宽屏）时默认展开目录；用户手动收起后不再打扰
  $effect(() => {
    const doc = nav.current;
    if (!doc || narrow || tocTouched) return;
    if (typeof window !== 'undefined' && window.innerWidth >= TOC_AUTO_OPEN_MIN_WIDTH) {
      tocOpen = true;
    }
  });

  // 响应式窄屏判定（替代旧 isNarrow() 闭包，让 Svelte 重渲染）
  let narrow = $state(false);
  function updateNarrow() {
    if (typeof window === 'undefined') return false;
    const coarse = window.matchMedia('(pointer: coarse)').matches;
    const w = Math.min(
      window.innerWidth || Infinity,
      window.visualViewport?.width ?? Infinity,
      window.screen?.width ?? Infinity
    );
    return w <= 768 || coarse;
  }

  // Tab Bar 当前激活项（窄屏抽屉的统一入口）
  let sidebarOpen = $state(true);

  async function pickVault() {
    const ok = await vault.pick();
    if (ok && narrow) {
      sidebarOpen = true;
      drawer = 'none';
    }
  }

  /** 底部 Tab Bar 点击：库 / 目录 / 搜索 / 书签 / 设置 */
  type TabId = 'library' | 'toc' | 'search' | 'bookmarks' | 'settings';
  function onTabClick(id: TabId) {
    if (id === 'library') {
      // 库：切换侧栏可见性
      sidebarOpen = !sidebarOpen;
      if (sidebarOpen) drawer = 'none';
      return;
    }
    // 其余四个：互斥抽屉
    if (drawer === id) {
      drawer = 'none';
    } else {
      drawer = id as DrawerKind;
      sidebarOpen = false;
    }
  }

  /** 用 Tab Bar 命中当前抽屉状态（高亮） */
  function tabActive(id: TabId): boolean {
    if (id === 'library') return sidebarOpen && drawer === 'none';
    return drawer === id;
  }

  onMount(() => {
    narrow = updateNarrow();
    const mql = window.matchMedia('(max-width: 768px), (pointer: coarse)');
    const onChange = () => {
      narrow = updateNarrow();
      // 窄屏默认收起侧栏；宽屏默认展开
      sidebarOpen = !narrow;
      // 目录在两种布局里是不同的容器：抽屉 → 侧栏，切换时迁移状态
      if (!narrow && drawer === 'toc') {
        drawer = 'none';
        tocOpen = true;
      }
    };
    mql.addEventListener('change', onChange);
    sidebarOpen = !narrow;

    let unlistenShare: (() => void) | undefined;
    let unlistenDeepLink: (() => void) | undefined;
    void (async () => {
      await prefs.hydrate();
      const last = await lastVault();
      // 持久层失败要一次性告知（改进计划 B5）：否则搜索/进度/书签只是"没反应"
      if (!dbStatus().available) {
        vault.showToast('本地数据库不可用：跨文件搜索、阅读进度与书签已关闭');
      }
      if (last) await vault.setRoot(last.root_uri);
      unlistenShare = await nav.listenForShare();
      unlistenDeepLink = await nav.listenForDeepLink();

      // 桌面：双击 md 文件冷启动，取走后端暂存的路径并打开
      const pending = await api.takePendingShare();
      if (pending) void nav.openShared(pending);

      // 移动端安全区探测（顶部 + 底部），env() 不稳定时按 Android 标准尺寸兜底
      if (await api.isMobile()) {
        const probe = document.createElement('div');
        probe.style.paddingTop = 'env(safe-area-inset-top, 0px)';
        probe.style.paddingBottom = 'env(safe-area-inset-bottom, 0px)';
        // 不可见、脱离正常文档流
        probe.style.position = 'absolute';
        probe.style.visibility = 'hidden';
        probe.style.pointerEvents = 'none';
        document.body.appendChild(probe);
        const cs = getComputedStyle(probe);
        const satPx = parseFloat(cs.paddingTop);
        const sabPx = parseFloat(cs.paddingBottom);
        probe.remove();
        // 兜底：Android 状态栏 24px、导航/手势条 18px
        const safeTop = satPx > 0 ? cs.paddingTop : '24px';
        const safeBottom = sabPx > 0 ? cs.paddingBottom : '18px';
        document.documentElement.style.setProperty('--safe-top', safeTop);
        document.documentElement.style.setProperty('--safe-bottom', safeBottom);
      }
    })();
    return () => {
      mql.removeEventListener('change', onChange);
      unlistenShare?.();
      unlistenDeepLink?.();
    };
  });

  // 返回键 / 浏览器后退
  function onKeydown(e: KeyboardEvent) {
    // Ctrl/Cmd+Shift+T：展开/收起右侧目录栏（仅宽屏有效）
    if ((e.ctrlKey || e.metaKey) && e.shiftKey && e.key.toLowerCase() === 't') {
      e.preventDefault();
      if (!narrow && nav.current) toggleToc();
      return;
    }
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
    if (narrow && sidebarOpen) { sidebarOpen = false; return true; }
    return nav.back();
  }
  function closeViewer() { viewerSrc = null; }
  function closeDrawer() { drawer = 'none'; }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="layout" class:is-narrow={narrow}>
  {#if narrow}
    <!-- ========== 窄屏：Large Title 顶栏 + 底部 Tab Bar ========== -->
    <header class="ios-titlebar">
      <div class="ios-titlebar-nav">
        {#if nav.current}
          <button class="icon-btn ios-icon" title="返回" disabled={!nav.canGoBack} onclick={() => nav.back()}>
            <Icon name="back" size={20} />
          </button>
        {:else}
          <button class="icon-btn ios-icon" title="侧栏" onclick={() => (sidebarOpen = !sidebarOpen)}>
            <Icon name="menu" size={20} />
          </button>
        {/if}
        <div class="ios-titlebar-actions">
          <button class="icon-btn ios-icon" title="查找" class:active={find.open} onclick={() => find.toggle()}>
            <Icon name="find" size={20} />
          </button>
          <button class="icon-btn ios-icon" title="设置" class:active={drawer === 'settings'} onclick={() => onTabClick('settings')}>
            <Icon name="settings" size={20} />
          </button>
        </div>
      </div>
      <div class="ios-titlebar-title">
        {#if nav.current}
          {nav.current.result.title ?? nav.current.name}
        {:else if vault.root}
          {vault.name}
        {/if}
      </div>
    </header>

    <main class="main-area ios-main">
      {#if vault.indexing && vault.indexProgress}
        <div class="index-banner">
          正在建立搜索索引… {vault.indexProgress.done}/{vault.indexProgress.total}
        </div>
      {/if}
      {#if nav.current}
        <!-- keyed：切换文档时强制重挂载 ArticleView（改进计划 B6）。
             否则上一篇文档尚未结束的 async 尾段（进度滚动、renderedCount 赋值）
             会写到新文档的 DOM 上。 -->
        {#key nav.current?.entry}
          <ArticleView doc={nav.current} onOpenImage={(src) => (viewerSrc = src)} />
        {/key}
      {:else}
        <Welcome />
      {/if}
    </main>

    <nav class="ios-tabbar" aria-label="主导航">
      <button class="tab" class:active={tabActive('library')} onclick={() => onTabClick('library')}>
        <Icon name="library" size={24} />
        <span>库</span>
      </button>
      <button class="tab" class:active={tabActive('toc')} disabled={!nav.current} onclick={() => onTabClick('toc')}>
        <Icon name="toc" size={24} />
        <span>目录</span>
      </button>
      <button class="tab" class:active={tabActive('search')} onclick={() => onTabClick('search')}>
        <Icon name="search" size={24} />
        <span>搜索</span>
      </button>
      <button class="tab" class:active={tabActive('bookmarks')} onclick={() => onTabClick('bookmarks')}>
        <Icon name="star" size={24} />
        <span>书签</span>
      </button>
      <button class="tab" class:active={tabActive('settings')} onclick={() => onTabClick('settings')}>
        <Icon name="settings" size={24} />
        <span>设置</span>
      </button>
    </nav>
  {:else}
    <!-- ========== 宽屏（桌面）：侧栏 + 顶 toolbar ========== -->
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
        onOpenFile={() => (sidebarOpen = false)}
        onOpenImage={(src) => (viewerSrc = src)}
      />
    </aside>

    <!-- 桌面端文件树是 in-flow 的（不是浮层），因此不需要遮罩。
         原先的 .sidebar-backdrop 会把正文与右侧目录栏一起压暗，并拦截所有点击。 -->
    <main class="main-area">
      <div class="toolbar">
        <button class="icon-btn" title="侧栏" onclick={() => (sidebarOpen = !sidebarOpen)}>
          <Icon name="menu" />
        </button>
        <button class="icon-btn" title="返回" disabled={!nav.canGoBack} onclick={() => nav.back()}>
          <Icon name="back" />
        </button>
        {#if nav.current}
          <span class="doc-title">{nav.current.result.title ?? nav.current.name}</span>
        {/if}
        <span class="spacer"></span>
        <div class="toolbar-group">
          <button class="icon-btn" class:active={tocOpen} title="目录 (Ctrl+Shift+T)" onclick={toggleToc}>
            <Icon name="toc" />
          </button>
          <button class="icon-btn" class:active={drawer === 'search'} title="搜索" onclick={() => (drawer = drawer === 'search' ? 'none' : 'search')}>
            <Icon name="search" />
          </button>
          <button class="icon-btn" class:active={find.open} title="文档内查找 (Ctrl+F)" onclick={() => find.toggle()}>
            <Icon name="find" />
          </button>
          <button class="icon-btn" class:active={drawer === 'bookmarks'} title="书签" onclick={() => (drawer = drawer === 'bookmarks' ? 'none' : 'bookmarks')}>
            <Icon name="star" />
          </button>
          <button class="icon-btn" class:active={drawer === 'settings'} title="设置" onclick={() => (drawer = drawer === 'settings' ? 'none' : 'settings')}>
            <Icon name="settings" />
          </button>
        </div>
      </div>

      {#if vault.indexing && vault.indexProgress}
        <div class="index-banner">
          正在建立搜索索引… {vault.indexProgress.done}/{vault.indexProgress.total}
        </div>
      {/if}

      {#if nav.current}
        <!-- keyed：切换文档时强制重挂载 ArticleView（改进计划 B6） -->
        {#key nav.current?.entry}
          <ArticleView doc={nav.current} onOpenImage={(src) => (viewerSrc = src)} />
        {/key}
      {:else}
        <Welcome />
      {/if}
    </main>

    <!-- 右侧目录栏：flex 兄弟节点，与正文同层；展开时正文自动让出宽度 -->
    {#if tocOpen}
      <aside class="toc-panel" aria-label="文档目录">
        <TocDrawer onClose={closeToc} closeOnJump={false} />
      </aside>
    {/if}
  {/if}
</div>

<!-- ========== 抽屉（窄屏 = 全屏 Sheet；桌面 = 右侧滑入） ========== -->
{#if drawer !== 'none'}
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawer}></div>
  <div class="drawer" class:full-sheet={narrow}>
    {#if drawer === 'toc'}
      <TocDrawer onClose={closeDrawer} />
    {:else if drawer === 'search'}
      <SearchPanel onClose={closeDrawer} />
    {:else if drawer === 'bookmarks'}
      <BookmarksPanel onClose={closeDrawer} />
    {:else if drawer === 'settings'}
      <SettingsPanel onClose={closeDrawer} />
    {/if}
  </div>
{/if}

{#if viewerSrc}
  <ImageViewer src={viewerSrc} onClose={closeViewer} />
{/if}

{#if vault.toast}
  <div class="toast">{vault.toast}</div>
{/if}

<style>
  /* ============== 窄屏（iOS 风）样式 ============== */

  /* iOS Large Title 顶栏
     - 状态栏区域透出 WebView 背景（不贴边）
     - 底部无边框（内容滚动时才出现的细线由 .ios-main 负责） */
  .ios-titlebar {
    flex-shrink: 0;
    padding-top: var(--safe-top);
    background: var(--ui-bg);
    /* 无边框：状态栏区域与背景融合 */
  }
  .ios-titlebar-nav {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 16px 6px; /* 顶部多留 4px，让 Large Title 与状态栏呼吸 */
    min-height: 44px;
  }
  .ios-titlebar-actions { display: flex; gap: 4px; }
  .ios-icon {
    width: 40px;
    height: 40px;
    border-radius: 12px;
    color: var(--ui-fg-muted);
  }
  .ios-icon:active { transform: scale(0.88); }
  .ios-icon.active {
    background: color-mix(in srgb, var(--ui-accent) 15%, transparent);
    color: var(--ui-accent);
  }

  /* iOS Large Title 规范：34px / 行距 41pt / 字重 700 / 负字距 */
  .ios-titlebar-title {
    padding: 12px 20px 18px; /* 顶部再多 4px，让字母 baseline 与状态栏拉开 */
    font-size: 34px;
    font-weight: 700;
    letter-spacing: -0.028em;
    line-height: 1.206; /* 41/34 */
    color: var(--ui-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    /* 空标题时也保持最小高度（占位） */
    min-height: 58px;
  }
  .ios-titlebar-title:empty {
    padding: 0;
    min-height: 8px;
  }

  .ios-main {
    flex: 1;
    min-height: 0;
    background: var(--ui-bg);
  }

  /* iOS 底部 Tab Bar：强化毛玻璃 */
  .ios-tabbar {
    flex-shrink: 0;
    display: flex;
    justify-content: space-around;
    align-items: stretch;
    height: var(--tabbar-h);
    padding-bottom: var(--safe-bottom);
    /* 强化毛玻璃：更通透 */
    background: color-mix(in srgb, var(--ui-bg) 72%, transparent);
    border-top: 0.5px solid color-mix(in srgb, var(--ui-fg) 12%, transparent);
    -webkit-backdrop-filter: saturate(200%) blur(28px);
    backdrop-filter: saturate(200%) blur(28px);
  }
  .ios-tabbar .tab {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 3px;
    padding: 6px 0;
    border: none;
    background: transparent;
    color: var(--ui-fg-muted);
    font-size: 10px;
    font-weight: 500;
    cursor: pointer;
    transition: color 0.15s ease, transform 0.1s ease;
    letter-spacing: 0.02em;
    -webkit-tap-highlight-color: transparent;
  }
  .ios-tabbar .tab:active { transform: scale(0.92); }
  .ios-tabbar .tab.active { color: var(--ui-accent); }
  .ios-tabbar .tab:disabled {
    opacity: 0.32;
    cursor: default;
    pointer-events: none;
  }
</style>