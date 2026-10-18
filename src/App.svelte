<script lang="ts">
  import { onMount, untrack } from 'svelte';
  import { getCurrentWebview } from '@tauri-apps/api/webview';
  import { getCurrentWindow } from '@tauri-apps/api/window';
  import { prefs } from './lib/stores/prefs.svelte';
  import { vault } from './lib/stores/vault.svelte';
  import { nav } from './lib/stores/nav.svelte';
  import { find } from './lib/stores/find.svelte';
  import { reading } from './lib/stores/reading.svelte';
  import {
    lastVault,
    dbStatus,
    lastReadDoc,
    recentVaults,
    hideVault,
    type VaultRow,
  } from './lib/db';
  import { entryName, displayPath } from './lib/display';
  import { api } from './lib/api';
  import FileTree from './components/FileTree.svelte';
  import ArticleView from './components/ArticleView.svelte';
  import TocDrawer from './components/TocDrawer.svelte';
  import SearchPanel from './components/SearchPanel.svelte';
  import SettingsPanel from './components/SettingsPanel.svelte';
  import BookmarksPanel from './components/BookmarksPanel.svelte';
  import ImageViewer from './components/ImageViewer.svelte';
  import TypeControls from './components/TypeControls.svelte';
  import SourceView from './components/SourceView.svelte';
  import SplitView from './components/SplitView.svelte';
  import Welcome from './components/Welcome.svelte';
  import Icon from './components/Icons.svelte';

  type DrawerKind = 'none' | 'library' | 'toc' | 'search' | 'bookmarks' | 'settings';
  let drawer = $state<DrawerKind>('none');
  let viewerSrc = $state<string | null>(null);
  /** 视图：阅读 / 只读源码 / 并排对照（临时状态，切换文档回到阅读） */
  let viewMode = $state<'read' | 'source' | 'split'>('read');
  let sourceMode = $derived(viewMode !== 'read');
  let splitMode = $derived(viewMode === 'split');
  /** 拖拽文件/文件夹进窗口时的提示 */
  let dropActive = $state(false);
  /** 侧栏库名下拉：最近打开过的文档库 */
  let libraryMenu = $state(false);
  let recentLibs = $state<VaultRow[]>([]);

  async function refreshLibraryList() {
    try {
      recentLibs = await recentVaults(8);
    } catch {
      recentLibs = [];
    }
  }

  function toggleLibraryMenu() {
    libraryMenu = !libraryMenu;
    if (libraryMenu) void refreshLibraryList();
  }

  /** 切回最近用过的库：省掉「系统选择器 → 从根目录重新导航」 */
  async function switchLibrary(lib: VaultRow) {
    libraryMenu = false;
    if (lib.root_uri === vault.root) return;
    await vault.setRoot(lib.root_uri);
    void refreshLibraryList();
    // 权限失效时 setRoot 会中止并提示，此时不要清掉当前文档
    if (vault.root !== lib.root_uri) return;

    // 旧库的文档不能继续留着（面包屑会指向别的库，返回键还会跳过去）
    nav.clear();
    const last = await lastReadDoc(lib.id).catch(() => null);
    if (last) {
      try {
        await nav.open(last.entry, entryName(last.entry));
      } catch {
        /* 文件可能已被移动或删除，停在欢迎页 */
      }
    }
  }

  /** 从最近列表移除（只清展示记录，不删进度/书签） */
  async function removeLibrary(lib: VaultRow) {
    try {
      await hideVault(lib.id);
      await refreshLibraryList();
      vault.showToast(`已从列表移除「${lib.name}」，阅读进度与书签保留`);
    } catch (e) {
      vault.showError(e);
    }
  }

  /** 拖拽打开：md 文件直接阅读，目录直接作为文档库 */
  async function handleDrop(paths: string[]) {
    const first = paths.find(Boolean);
    if (!first) return;
    if (/\.(md|markdown|mdx)$/i.test(first)) {
      try {
        await nav.open(first, entryName(first));
      } catch (e) {
        vault.showError(e);
      }
      return;
    }
    await vault.setRoot(first);
  }

  /**
   * 宽屏右侧大纲：与正文同层（flex 兄弟节点），默认展开、可收起。
   * 窄屏仍走全屏抽屉（空间不够，侧栏会把正文压得太窄）。
   */
  let tocOpen = $state(true);
  /** 用户手动切换过目录栏后，不再自动干预 */
  let tocTouched = false;
  /** 窗口宽度低于该值时不展开大纲（展开会挤压正文） */
  const TOC_AUTO_OPEN_MIN_WIDTH = 1100;

  function toggleToc() {
    setToc(!tocOpen);
  }
  function closeToc() {
    setToc(false);
  }

  /**
   * 切换大纲时保持阅读位置：文内 [TOC] 会随大纲一起显隐，
   * 正文高度瞬间变化几千像素，不把当前章节拉回来，读者会"跳走"。
   */
  function setToc(next: boolean) {
    if (tocOpen === next) return;
    tocOpen = next;
    tocTouched = true;
    const id = reading.anchor;
    if (!id) return;
    requestAnimationFrame(() => {
      document.getElementById(id)?.scrollIntoView({ block: 'start', behavior: 'instant' });
    });
  }

  // 首次打开文档（或从窄屏切到宽屏）时默认展开目录；用户手动收起后不再打扰
  $effect(() => {
    const doc = nav.current;
    if (!doc || narrow || tocTouched) return;
    if (typeof window !== 'undefined' && window.innerWidth >= TOC_AUTO_OPEN_MIN_WIDTH) {
      tocOpen = true;
    }
  });

  /** 拖拽分栏：跟手移动，抬手时才落盘（拖拽中每帧写配置文件太浪费） */
  function startResize(kind: 'rail' | 'outline', e: PointerEvent) {
    e.preventDefault();
    const startX = e.clientX;
    const startW = kind === 'rail' ? prefs.railWidth : prefs.outlineWidth;
    document.body.classList.add('resizing');

    const onMove = (ev: PointerEvent) => {
      const dx = ev.clientX - startX;
      if (kind === 'rail') prefs.railWidth = startW + dx;
      else prefs.outlineWidth = startW - dx;
    };
    const onUp = () => {
      window.removeEventListener('pointermove', onMove);
      window.removeEventListener('pointerup', onUp);
      document.body.classList.remove('resizing');
      prefs.persistLayout();
    };
    window.addEventListener('pointermove', onMove);
    window.addEventListener('pointerup', onUp);
  }

  /** 双击分隔条复位默认宽度 */
  function resetWidth(kind: 'rail' | 'outline') {
    if (kind === 'rail') prefs.railWidth = 258;
    else prefs.outlineWidth = 288;
    prefs.persistLayout();
  }

  /** 窗口尺寸变化时重新评估（用户手动切换过就不再干预） */
  function syncOutlineWithWidth() {
    if (tocTouched || narrow) return;
    const shouldOpen = window.innerWidth >= TOC_AUTO_OPEN_MIN_WIDTH;
    if (shouldOpen !== tocOpen) {
      tocOpen = shouldOpen;
      const id = reading.anchor;
      if (id) {
        requestAnimationFrame(() => {
          document.getElementById(id)?.scrollIntoView({ block: 'start', behavior: 'instant' });
        });
      }
    }
  }

  /**
   * 大纲真正可见时，屏幕上隐藏文内 [TOC]（两者内容重复，且那张卡片常有 2–3 屏高）。
   * 只作用于 screen：导出 PDF 走打印媒体，仍保留文内目录。
   */
  $effect(() => {
    const visible = !narrow && tocOpen && !!nav.current;
    if (typeof document !== 'undefined') {
      document.documentElement.dataset.outline = visible ? 'open' : 'closed';
    }
  });

  // 窗口标题跟随当前文档：Alt-Tab / 任务栏才分得清在看哪篇
  $effect(() => {
    const current = nav.current;
    const title = current
      ? `${current.result.title ?? current.name} · Lnkwell`
      : 'Lnkwell';
    if (typeof document !== 'undefined') document.title = title;
    void getCurrentWindow().setTitle(title).catch(() => {});
  });

  // 换文档时回到渲染视图：源码只是「看一眼原文」，不该粘在下一篇。
  // 注意必须 untrack —— 直接读 sourceMode 会让这个 effect 依赖它，
  // 打开源码的瞬间就被自己重置回 false。
  $effect(() => {
    const entry = nav.current?.entry;
    untrack(() => {
      if (entry && viewMode !== 'read') viewMode = 'read';
    });
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
    // 窄屏没有常驻侧栏：选完库直接把「文档库」面板打开给用户看结果
    if (ok && narrow) {
      drawer = 'library';
      sidebarOpen = false;
    }
  }

  /** 底部 Tab Bar 点击：库 / 目录 / 搜索 / 书签 / 设置（窄屏） */
  type TabId = 'library' | 'toc' | 'search' | 'bookmarks' | 'settings';
  function onTabClick(id: TabId) {
    // 互斥抽屉：同一个再点一次收起
    if (drawer === id) {
      drawer = 'none';
      return;
    }
    drawer = id as DrawerKind;
    sidebarOpen = false;
  }

  /** 用 Tab Bar 命中当前抽屉状态（高亮） */
  function tabActive(id: TabId): boolean {
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
    // 启动即按当前窗口宽度定夺大纲（避免"先展开再收起"的闪动）
    syncOutlineWithWidth();

    // 窗口尺寸变化 → 重新评估大纲是否该自动展开（只在用户没手动切换过时生效）
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    const onResize = () => {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => syncOutlineWithWidth(), 120);
    };
    window.addEventListener('resize', onResize);

    let unlistenShare: (() => void) | undefined;
    let unlistenDeepLink: (() => void) | undefined;
    let unlistenDrop: (() => void) | undefined;
    let unlistenPop: (() => void) | undefined;
    void (async () => {
      await prefs.hydrate();
      const last = await lastVault();
      // 持久层失败要一次性告知（改进计划 B5）：否则搜索/进度/书签只是"没反应"
      if (!dbStatus().available) {
        vault.showToast('本地数据库不可用：跨文件搜索、阅读进度与书签已关闭');
      }
      if (last) await vault.setRoot(last.root_uri);
      void refreshLibraryList();
      unlistenShare = await nav.listenForShare();
      unlistenDeepLink = await nav.listenForDeepLink();

      // 桌面：双击 md 文件冷启动，取走后端暂存的路径并打开
      const pending = await api.takePendingShare();
      if (pending) {
        void nav.openShared(pending);
      } else if (last) {
        // 启动续读：直接回到上次那篇文档（ArticleView 会自动恢复到保存的锚点）
        const resume = await lastReadDoc(last.id).catch(() => null);
        if (resume) {
          void nav.open(resume.entry, entryName(resume.entry)).catch(() => {
            /* 文件可能已被移动/删除，静默回落到 Welcome */
          });
        }
      }

      // 拖拽打开（桌面端）
      try {
        unlistenDrop = await getCurrentWebview().onDragDropEvent((event) => {
          const p = event.payload;
          if (p.type === 'enter' || p.type === 'over') {
            dropActive = true;
          } else if (p.type === 'drop') {
            dropActive = false;
            void handleDrop(p.paths);
          } else {
            dropActive = false;
          }
        });
      } catch {
        /* 非 Tauri 环境（浏览器预览）没有该 API */
      }

      // 安卓物理返回键：WebView 没有历史记录时按返回会直接退出 App。
    // 压一个哨兵历史项，让返回先变成 popstate —— 由 handleBack() 决定
    // 是关面板、返回上一篇，还是真的退出（没得处理时不再压栈）。
    if (narrow) {
      history.pushState({ lnkwell: true }, '');
      const onPop = () => {
        if (handleBack()) history.pushState({ lnkwell: true }, '');
      };
      window.addEventListener('popstate', onPop);
      unlistenPop = () => window.removeEventListener('popstate', onPop);
    }

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
      window.removeEventListener('resize', onResize);
      clearTimeout(resizeTimer);
      unlistenShare?.();
      unlistenDeepLink?.();
      unlistenDrop?.();
      unlistenPop?.();
    };
  });

  // 返回键 / 浏览器后退
  function onKeydown(e: KeyboardEvent) {
    const mod = e.ctrlKey || e.metaKey;

    // Ctrl/Cmd+O：打开文件夹
    if (mod && !e.shiftKey && e.key.toLowerCase() === 'o') {
      e.preventDefault();
      void pickVault();
      return;
    }
    // Ctrl/Cmd+/：切换只读源码视图
    if (mod && (e.key === '/' || e.code === 'Slash')) {
      e.preventDefault();
      if (nav.current) viewMode = viewMode === 'read' ? 'source' : 'read';
      return;
    }
    // 字号：Ctrl + / - / 0（阅读器里最高频的个性化操作）
    if (mod && ['=', '+', '-', '_', '0'].includes(e.key)) {
      e.preventDefault();
      // Ctrl+0 复位成"跟随主题"，正负号从当前生效值起算
      if (e.key === '0') prefs.fontSize = null;
      else if (e.key === '-' || e.key === '_')
        prefs.fontSize = Math.max(13, prefs.effectiveFontSize() - 1);
      else prefs.fontSize = Math.min(24, prefs.effectiveFontSize() + 1);
      return;
    }
    // Ctrl/Cmd+K：全局搜索
    if (mod && !e.shiftKey && e.key.toLowerCase() === 'k') {
      e.preventDefault();
      drawer = drawer === 'search' ? 'none' : 'search';
      return;
    }
    // Ctrl/Cmd+Shift+T：展开/收起右侧大纲（仅宽屏有效）
    if (mod && e.shiftKey && e.key.toLowerCase() === 't') {
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
    if (viewMode !== 'read') { viewMode = 'read'; return true; }
    if (drawer !== 'none') { drawer = 'none'; return true; }
    if (narrow && sidebarOpen) { sidebarOpen = false; return true; }
    return nav.back();
  }
  function closeViewer() { viewerSrc = null; }
  function closeDrawer() { drawer = 'none'; }
</script>

<svelte:window onkeydown={onKeydown} />

<!-- 文档库面板：桌面作为常驻资料栏，窄屏作为「库」抽屉的内容（同一份模板） -->
{#snippet libraryPanel()}
  <div class="rail-head">
    <span class="rail-mark" aria-hidden="true">L</span>
    <button
      class="rail-vault"
      title="切换文档库"
      aria-haspopup="menu"
      aria-expanded={libraryMenu}
      onclick={toggleLibraryMenu}
    >
      <span class="rail-vault-name">{vault.name || 'Lnkwell'}</span>
      <span class="rail-vault-sub">
        {vault.root ? `${vault.topNodes.length} 个条目` : '本地 Markdown 库'}
      </span>
      <Icon name="down" size={12} />
    </button>
    <div class="rail-actions">
      <button class="icon-btn" title="刷新目录" onclick={() => vault.refresh()}>
        <Icon name="refresh" size={15} />
      </button>
      <button class="icon-btn" title="更换文档库" onclick={pickVault}>
        <Icon name="folder" size={15} />
      </button>
    </div>
  </div>

  {#if libraryMenu}
    <button class="menu-backdrop" aria-label="关闭" onclick={() => (libraryMenu = false)}></button>
    <div class="library-menu" role="menu">
      <div class="library-menu-head">最近文档库</div>
      {#each recentLibs as lib (lib.id)}
        <div class="library-row">
          <button
            class="library-item"
            class:active={lib.root_uri === vault.root}
            onclick={() => switchLibrary(lib)}
          >
            <span class="library-name">{lib.name}</span>
            <span class="library-path">{displayPath(lib.root_uri)}</span>
          </button>
          {#if lib.root_uri !== vault.root}
            <button
              class="library-remove"
              title="从列表移除（进度与书签保留）"
              aria-label={`从列表移除 ${lib.name}`}
              onclick={(e) => {
                e.stopPropagation();
                void removeLibrary(lib);
              }}
            >
              <Icon name="close" size={12} />
            </button>
          {/if}
        </div>
      {/each}
      {#if recentLibs.length === 0}
        <div class="library-empty">还没有打开过文档库</div>
      {/if}
      <div class="library-sep"></div>
      <button
        class="library-item library-add"
        onclick={() => {
          libraryMenu = false;
          void vault.pick();
        }}
      >
        <Icon name="folder" size={13} />
        打开其他文件夹…
      </button>
    </div>
  {/if}

  <button class="rail-search" title="搜索文档 (Ctrl+K)" onclick={() => (drawer = 'search')}>
    <Icon name="search" size={14} />
    <span>搜索文档</span>
    <span class="kbd">Ctrl K</span>
  </button>

  <!-- 书签是「内容导航」，和搜索同属一级，放在资料栏导航区；
       设置是「全局偏好」，钉在底部（VS Code / Obsidian / Notion 都是这个分工）。 -->
  <button
    class="rail-nav"
    class:active={drawer === 'bookmarks'}
    title="书签"
    onclick={() => (drawer = drawer === 'bookmarks' ? 'none' : 'bookmarks')}
  >
    <Icon name="star" size={14} />
    <span>书签</span>
  </button>

  <div class="rail-section">文档</div>
  <FileTree
    onOpenImage={(src) => (viewerSrc = src)}
    onOpenFile={() => {
      if (narrow) drawer = 'none';
    }}
  />

  <div class="rail-foot">
    <button
      class="rail-foot-btn"
      class:active={drawer === 'settings'}
      onclick={() => (drawer = drawer === 'settings' ? 'none' : 'settings')}
    >
      <Icon name="settings" size={15} />设置
    </button>
  </div>
{/snippet}

<div class="layout" class:is-narrow={narrow}>
  {#if narrow}
    <!-- ========== 窄屏：Large Title 顶栏 + 底部 Tab Bar ========== -->
    <header class="ios-titlebar">
      <div class="ios-titlebar-nav">
        {#if nav.current}
          <button class="icon-btn ios-icon" title="返回" disabled={!nav.canGoBack} onclick={() => nav.back()}>
            <Icon name="back" size={20} />
          </button>
          <!-- 阅读时标题收进导航栏：正文 H1 才是视觉标题，
               再放一个 34px 大标题会和它重复，还白占两屏高度 -->
          <div class="ios-titlebar-inline">{nav.current.result.title ?? nav.current.name}</div>
        {:else}
          <button class="icon-btn ios-icon" title="文档库" onclick={() => onTabClick('library')}>
            <Icon name="library" size={20} />
          </button>
        {/if}
        <div class="ios-titlebar-actions">
          <button
            class="icon-btn ios-icon"
            title="查找"
            class:active={find.open}
            disabled={sourceMode}
            onclick={() => find.toggle()}
          >
            <Icon name="find" size={20} />
          </button>
          <button
            class="icon-btn ios-icon"
            title="源码"
            class:active={sourceMode}
            disabled={!nav.current}
            onclick={() => (viewMode = viewMode === 'read' ? 'source' : 'read')}
          >
            <Icon name="code" size={20} />
          </button>
          <button class="icon-btn ios-icon" title="设置" class:active={drawer === 'settings'} onclick={() => onTabClick('settings')}>
            <Icon name="settings" size={20} />
          </button>
        </div>
      </div>
      {#if !nav.current}
        <div class="ios-titlebar-title">{vault.root ? vault.name : ''}</div>
      {/if}
    </header>

    <main class="main-area ios-main">
      {#if vault.indexing && vault.indexProgress}
        <div class="index-banner">
          正在建立搜索索引… {vault.indexProgress.done}/{vault.indexProgress.total}
        </div>
      {/if}
      {#if nav.current}
        {#if sourceMode}
          <!-- 窄屏不做并排对照：两栏各 200px 没有可读性 -->
          <SourceView doc={nav.current} onExit={() => (viewMode = 'read')} />
        {:else}
          <!-- keyed：切换文档时强制重挂载 ArticleView（改进计划 B6）。
               否则上一篇文档尚未结束的 async 尾段（进度滚动、renderedCount 赋值）
               会写到新文档的 DOM 上。 -->
          {#key nav.current?.entry}
            <ArticleView doc={nav.current} onOpenImage={(src) => (viewerSrc = src)} />
          {/key}
        {/if}
      {:else}
        <Welcome
          onPicked={() => {
            if (narrow) drawer = 'library';
          }}
        />
      {/if}
    </main>

    <nav class="ios-tabbar" aria-label="主导航">
      <button class="tab" class:active={tabActive('library')} onclick={() => onTabClick('library')}>
        <Icon name="library" size={24} />
        <span>库</span>
      </button>
      <button class="tab" class:active={tabActive('toc')} disabled={!nav.current || sourceMode} onclick={() => onTabClick('toc')}>
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
      <!-- 设置不占 Tab：它是全局偏好，不是一级导航。
           入口只留标题栏齿轮，与桌面端「设置钉在侧栏底部」的分工一致；
           底部 4 项也让 411dp 宽度下的每格更好点。 -->
    </nav>
  {:else}
    <!-- ========== 宽屏（桌面）：资料栏 / 阅读区 / 大纲 ========== -->
    <aside class="rail" class:collapsed={!sidebarOpen}>
      {@render libraryPanel()}
      {#if sidebarOpen}
        <div
          class="resize-handle rail-handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="拖拽调整资料栏宽度"
          title="拖拽调整宽度，双击复位"
          onpointerdown={(e) => startResize('rail', e)}
          ondblclick={() => resetWidth('rail')}
        ></div>
      {/if}
    </aside>

    <main class="main-area">
      <header class="topbar">
        <button
          class="icon-btn"
          title={sidebarOpen ? '收起资料栏' : '展开资料栏'}
          onclick={() => (sidebarOpen = !sidebarOpen)}
        >
          <Icon name="panel-left" />
        </button>
        <button class="icon-btn" title="返回" disabled={!nav.canGoBack} onclick={() => nav.back()}>
          <Icon name="back" />
        </button>
        <nav class="crumb" aria-label="当前位置">
          {#if nav.current}
            <span class="crumb-vault">{vault.name || '文档库'}</span>
            <span class="crumb-sep">/</span>
            <span class="crumb-name" title={nav.current.name}>
              {nav.current.result.title ?? nav.current.name}
            </span>
          {:else if vault.name}
            <span class="crumb-name">{vault.name}</span>
          {/if}
        </nav>
        <span class="spacer"></span>
        <button
          class="icon-btn"
          class:active={find.open}
          title="文档内查找 (Ctrl+F)"
          onclick={() => find.toggle()}
        >
          <Icon name="find" />
        </button>
        <button
          class="icon-btn"
          class:active={sourceMode}
          title="源码 (Ctrl+/)"
          disabled={!nav.current}
          onclick={() => (viewMode = viewMode === 'read' ? 'source' : 'read')}
        >
          <Icon name="code" />
        </button>
        <button
          class="icon-btn"
          class:active={tocOpen}
          title="大纲 (Ctrl+Shift+T)"
          disabled={!nav.current || sourceMode}
          onclick={toggleToc}
        >
          <Icon name="panel-right" />
        </button>
      </header>

      {#if !sourceMode}
        <div class="progress-line" aria-hidden="true">
          <span style:width="{Math.round(reading.progress * 100)}%"></span>
        </div>
      {/if}

      {#if vault.indexing && vault.indexProgress}
        <div class="index-banner">
          正在建立搜索索引… {vault.indexProgress.done}/{vault.indexProgress.total}
        </div>
      {/if}

      {#if nav.current}
        {#if splitMode}
          <SplitView
            doc={nav.current}
            onOpenImage={(src) => (viewerSrc = src)}
            onSource={() => (viewMode = 'source')}
            onClose={() => (viewMode = 'read')}
          />
        {:else if sourceMode}
          <SourceView
            doc={nav.current}
            onExit={() => (viewMode = 'read')}
            onSplit={() => (viewMode = 'split')}
          />
        {:else}
          <!-- keyed：切换文档时强制重挂载 ArticleView（改进计划 B6） -->
          {#key nav.current?.entry}
            <ArticleView doc={nav.current} onOpenImage={(src) => (viewerSrc = src)} />
          {/key}
        {/if}
      {:else}
        <Welcome />
      {/if}
      {#if nav.current && !sourceMode}
        <TypeControls />
      {/if}
    </main>

    <!-- 右侧大纲：flex 兄弟节点，与正文同层；展开时正文让出宽度 -->
    {#if tocOpen && nav.current && !sourceMode}
      <aside class="outline" aria-label="文档大纲">
        <TocDrawer onClose={closeToc} closeOnJump={false} />
        <div
          class="resize-handle outline-handle"
          role="separator"
          aria-orientation="vertical"
          aria-label="拖拽调整大纲宽度"
          title="拖拽调整宽度，双击复位"
          onpointerdown={(e) => startResize('outline', e)}
          ondblclick={() => resetWidth('outline')}
        ></div>
      </aside>
    {/if}
  {/if}
</div>

{#if dropActive}
  <div class="drop-overlay" role="presentation">
    <div class="drop-card">
      <strong>松手即可打开</strong>
      <span>Markdown 文件，或整个文件夹作为文档库</span>
    </div>
  </div>
{/if}

<!-- ========== 抽屉（窄屏 = 全屏 Sheet；桌面 = 右侧滑入） ========== -->
{#if drawer !== 'none'}
  <div class="drawer-backdrop" role="presentation" onclick={closeDrawer}></div>
  <div class="drawer" class:full-sheet={narrow}>
    {#if drawer === 'library'}
      <div class="drawer-header">
        <span>文档库</span>
        <button class="icon-btn" title="关闭" onclick={closeDrawer}>
          <Icon name="close" size={14} />
        </button>
      </div>
      {@render libraryPanel()}
    {:else if drawer === 'toc'}
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
  <div class="toast" class:has-action={!!vault.toastAction}>
    <span class="toast-msg">{vault.toast}</span>
    {#if vault.toastAction}
      <button
        class="toast-action"
        onclick={() => {
          const action = vault.toastAction;
          vault.dismissToast();
          action?.run();
        }}
      >{vault.toastAction.label}</button>
    {/if}
  </div>
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
  /* 阅读态：标题缩进导航栏里（15px / 600），给正文让出空间 */
  .ios-titlebar-inline {
    flex: 1;
    min-width: 0;
    margin: 0 6px 0 2px;
    font-size: 15px;
    font-weight: 600;
    letter-spacing: -0.012em;
    color: var(--ui-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
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
    /* 高度必须加上安全区：box-sizing: border-box 下，只写 content 高度会把
       padding-bottom 吃进 50px 里，图标和文字被挤到手势条底下 */
    height: var(--tabbar-h-with-safe);
    padding-top: 6px;
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