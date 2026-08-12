import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api, errorMessage } from '../api';
import type { FileNode, PlainDoc } from '../api';
import { upsertVault, touchVault, indexDoc, type VaultRow } from '../db';

/**
 * 从 root 引用提取人类可读的库名。
 * 桌面是文件路径（取最后一段）；Android 是 FsUri JSON，
 * 其 uri 字段形如 content://.../tree/primary%3ADocuments%2Fmdvault，需解码取末段。
 */
function deriveName(root: string): string {
  let s = root;
  // Android：尝试解析 FsUri JSON，取 uri 字段
  if (root.startsWith('{')) {
    try {
      const o = JSON.parse(root);
      if (typeof o?.uri === 'string') s = o.uri;
    } catch {
      /* 保持原样 */
    }
  }
  // URL 解码（SAF tree URI 是 percent-encoded），失败则保持原样
  try {
    s = decodeURIComponent(s);
  } catch {
    /* 保持原样 */
  }
  // 取路径最后一段作为库名
  const seg = s.replace(/\\/g, '/').split('/').filter(Boolean).pop() ?? s;
  // content://.../tree/primary:Documents/mdvault → mdvault；若末段是 primary:Documents 这类，取冒号后
  return seg.includes(':') ? seg.split(':').pop() ?? seg : seg;
}

/** 文档库状态（方案 §4、§7.3） */
class Vault {
  root = $state<string | null>(null);
  name = $state<string>('');
  topNodes = $state<FileNode[]>([]);
  loading = $state(false);
  indexing = $state(false);
  indexProgress = $state<{ done: number; total: number } | null>(null);
  toast = $state<string | null>(null);

  #toastTimer: ReturnType<typeof setTimeout> | undefined;
  #vaultId = $state<number | null>(null);

  get vaultId() { return this.#vaultId; }

  showToast(msg: string) {
    this.toast = msg;
    clearTimeout(this.#toastTimer);
    this.#toastTimer = setTimeout(() => (this.toast = null), 2600);
  }

  /** 刷新当前库的目录树（不重选，重新 list_dir） */
  async refresh(): Promise<void> {
    if (!this.root) return;
    this.loading = true;
    try {
      this.topNodes = await api.listDir(this.root);
      this.showToast(`已刷新：${this.topNodes.length} 项`);
    } catch (e) {
      this.showToast(errorMessage(e));
    } finally {
      this.loading = false;
    }
  }

  /** 选择文件夹 → 校验权限 → 入库 → 列根目录 */
  async pick(): Promise<boolean> {
    try {
      const root = await api.pickVault();
      if (!root) return false;
      await this.setRoot(root);
      return true;
    } catch (e) {
      this.showToast(errorMessage(e));
      return false;
    }
  }

  async setRoot(root: string) {
    // SAF 权限检查（方案 §11.4）：失效时引导重选，不报错
    const ok = await api.checkVaultPermission(root).catch(() => true);
    if (!ok) {
      this.showToast('文档库访问权限已失效，请重新选择文件夹');
      return;
    }
    this.root = root;
    this.name = deriveName(root);
    this.loading = true;
    try {
      this.topNodes = await api.listDir(root);
      // 选库成功的可见反馈（窄屏侧栏收起时用户看不到目录树）
      if (this.topNodes.length > 0) {
        this.showToast(`已加载「${this.name}」：${this.topNodes.length} 项`);
      } else {
        this.showToast(`「${this.name}」里没有 Markdown 文件`);
      }
      const v: VaultRow | null = await upsertVault(this.name, root);
      if (v) {
        this.#vaultId = v.id;
        await touchVault(v.id);
      }
    } catch (e) {
      console.error('[lnkwell] listDir/db error', e);
      this.showToast(errorMessage(e));
    } finally {
      this.loading = false;
    }
  }

  /**
   * 后台建 FTS5 索引（方案 §8.5）。
   * Rust 侧逐篇 emit 'index-file'，这里监听并写入 SQLite；进度经 'index-progress' 回报。
   * 不阻塞 UI，单篇失败由 Rust 侧跳过。
   */
  async startIndexing(): Promise<void> {
    if (!this.root || this.indexing) return;
    // 确保有 vault_id（首次选库后 setRoot 已 upsert）
    if (this.#vaultId == null) {
      const v = await upsertVault(this.name, this.root);
      this.#vaultId = v?.id ?? null;
    }
    const vaultId = this.#vaultId;
    if (vaultId == null) {
      this.showToast('数据库不可用，无法建立索引');
      return;
    }

    this.indexing = true;
    this.indexProgress = { done: 0, total: 0 };

    const unlistens: UnlistenFn[] = [];
    try {
      unlistens.push(
        await listen<PlainDoc>('index-file', (ev) => {
          const p = ev.payload;
          void indexDoc(vaultId, p.entry, p.title, p.plain, p.mtime);
        }),
        await listen<[number, number]>('index-progress', (ev) => {
          this.indexProgress = { done: ev.payload[0], total: ev.payload[1] };
        })
      );
      await api.buildIndex(this.root);
    } catch (e) {
      this.showToast(errorMessage(e));
    } finally {
      for (const u of unlistens) u();
      this.indexing = false;
      this.indexProgress = null;
    }
  }
}

export const vault = new Vault();
