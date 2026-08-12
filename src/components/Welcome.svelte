<script lang="ts">
  import { onMount } from 'svelte';
  import { vault } from '../lib/stores/vault.svelte';
  import { nav } from '../lib/stores/nav.svelte';
  import { recentDocs, type RecentDoc } from '../lib/db';
  import { errorMessage } from '../lib/api';

  let recents = $state<RecentDoc[]>([]);

  onMount(async () => {
    try {
      recents = await recentDocs(8);
    } catch {
      recents = [];
    }
  });

  function baseName(p: string): string {
    return p.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? p;
  }

  function relTime(ts: number): string {
    const m = Math.floor((Date.now() - ts) / 60000);
    if (m < 1) return '刚刚';
    if (m < 60) return `${m} 分钟前`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h} 小时前`;
    const d = Math.floor(h / 24);
    if (d < 30) return `${d} 天前`;
    return new Date(ts).toLocaleDateString();
  }

  async function openRecent(r: RecentDoc) {
    try {
      if (vault.root !== r.vault_root) {
        await vault.setRoot(r.vault_root);
      }
      await nav.open(r.rel_path, baseName(r.rel_path), { replace: true });
    } catch (e) {
      vault.showToast(errorMessage(e));
    }
  }
</script>

<div class="welcome">
  <h1>Lnkwell</h1>
  <p class="welcome-tagline">离线优先的 Markdown 阅读器 —— 把本地文档渲染得和 GitHub 一样好看</p>
  <button class="primary-btn" onclick={() => vault.pick()}>选择文档库</button>

  {#if recents.length > 0}
    <div class="recent-list">
      <p class="recent-heading">最近打开</p>
      {#each recents as r (r.vault_root + r.rel_path)}
        <button class="recent-item" onclick={() => openRecent(r)}>
          <div>{r.title ?? baseName(r.rel_path)}</div>
          {#if r.excerpt}
            <div class="excerpt">{r.excerpt}</div>
          {/if}
          <div class="path">{r.rel_path} · {relTime(r.updated_at)}</div>
        </button>
      {/each}
    </div>
  {/if}
</div>
