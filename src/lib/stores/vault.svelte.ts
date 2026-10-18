import { api, errorMessage, isAppError } from '../api';
import type { FileNode } from '../api';
import { upsertVault, touchVault, indexDoc, docMtimes, pruneDocs, type VaultRow } from '../db';
import { entryName } from '../display';

/** 文档库状态（方案 §4、§7.3） */
class Vault {
  root = $state<string | null>(null);
  name = $state<string>('');
  topNodes = $state<FileNode[]>([]);
  loading = $state(false);
  indexing = $state(false);
  indexProgress = $state<{ done: number; total: number } | null>(null);
  toast = $state<string | null>(null);
  /** 提示里附带的可点操作（例如"重新选择文件夹"） */
  toastAction = $state<{ label: string; run: () => void } | null>(null);

  #toastTimer: ReturnType<typeof setTimeout> | undefined;
  #vaultId = $state<number | null>(null);

  get vaultId() { return this.#vaultId; }

  showToast(msg: string, action?: { label: string; run: () => void }) {
    this.toast = msg;
    this.toastAction = action ?? null;
    clearTimeout(this.#toastTimer);
    // 带操作的提示要留够时间，否则用户来不及点
    this.#toastTimer = setTimeout(() => this.dismissToast(), action ? 9000 : 2600);
  }

  dismissToast() {
    clearTimeout(this.#toastTimer);
    this.toast = null;
    this.toastAction = null;
  }

  /**
   * 统一错误提示：权限失效、文件丢失这类「用户能自己补救」的错误，
   * 直接在提示里给出动作，而不是只显示一句红字然后消失。
   */
  showError(e: unknown) {
    if (isAppError(e)) {
      if (e.code === 'PERMISSION_EXPIRED') {
        this.showToast(errorMessage(e), {
          label: '重新选择文件夹',
          run: () => void this.pick(),
        });
        return;
      }
      if (e.code === 'NOT_FOUND') {
        this.showToast(errorMessage(e), {
          label: '刷新目录',
          run: () => void this.refresh(),
        });
        return;
      }
    }
    this.showToast(errorMessage(e));
  }

  /** 刷新当前库的目录树（不重选，重新 list_dir） */
  async refresh(): Promise<void> {
    if (!this.root) return;
    this.loading = true;
    try {
      this.topNodes = await api.listDir(this.root);
      this.showToast(`已刷新：${this.topNodes.length} 项`);
    } catch (e) {
      this.showError(e);
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
      this.showError(e);
      return false;
    }
  }

  async setRoot(root: string) {
    // SAF 权限检查（方案 §11.4）：失效时引导重选，不报错
    const ok = await api.checkVaultPermission(root).catch(() => true);
    if (!ok) {
      this.showToast('文档库访问权限已失效', {
        label: '重新选择文件夹',
        run: () => void this.pick(),
      });
      return;
    }
    this.root = root;
    // 库名与 Android 的 FsUri JSON 解耦：统一走 display.entryName（改进计划 C2）
    this.name = entryName(root);
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
      // 选库后自动建索引（改进计划 A3）：搜索、进度、最近阅读都依赖 docs 行，
      // 而旧实现只在「搜索无结果」时才让用户手动触发，等于默认没有索引。
      void this.startIndexing();
    } catch (e) {
      console.error('[lnkwell] listDir/db error', e);
      this.showError(e);
    } finally {
      this.loading = false;
    }
  }

  /**
   * 后台建 FTS5 索引（方案 §8.5 / 改进计划 A3+A4+A7）。
   *
   * 改为**前端驱动**的顺序循环：Rust 只负责「收集文件列表」与「单篇抽取纯文本」，
   * 索引写库、进度、失败处理都在这一侧完成。
   * 这样做的原因（原实现的三个问题）：
   *   1. Rust 边 emit 边返回，前端 `void indexDoc(...)` 不 await → 索引"完成"时写入仍在飞行；
   *   2. 事件单向无背压，千篇库会把 IPC 队列打满；
   *   3. 单篇失败的错误被 `void` 吞掉，无法汇总给用户。
   *
   * 增量：mtime 未变则跳过（Android SAF 无稳定 mtime，会全量重建）。
   */
  /**
   * @param force true 时忽略 mtime 全量重建 —— 索引逻辑/抽取规则变化后，
   *              未改动的文件也需要重新抽取（否则「重新建立索引」按不动）
   */
  async startIndexing(force = false): Promise<void> {
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
    const root = this.root;

    this.indexing = true;
    this.indexProgress = { done: 0, total: 0 };
    try {
      const files = await api.collectMarkdownFiles(root);
      const total = files.length;
      this.indexProgress = { done: 0, total };
      const known = await docMtimes(vaultId);
      const keep = new Set<string>();
      let done = 0;
      let updated = 0;
      let skipped = 0;
      let failed = 0;

      for (const f of files) {
        // 中途切换文档库：立即停止，避免把旧库的文件写进新库
        if (this.root !== root) return;
        keep.add(f.entry);
        done++;
        // 增量：mtime 未变直接跳过（force 时全部重抽）
        if (!force && f.mtime != null && known.get(f.entry) === f.mtime) {
          skipped++;
        } else {
          try {
            const p = await api.extractPlain(f.entry);
            await indexDoc(vaultId, p.entry, p.title, p.plain, f.mtime ?? p.mtime ?? 0);
            updated++;
          } catch (e) {
            // 单篇失败不中断整批（方案 §8.5）
            failed++;
            console.warn('[index] 跳过', f.entry, e);
          }
        }
        if (done % 10 === 0 || done === total) {
          this.indexProgress = { done, total };
        }
      }

      const pruned = await pruneDocs(vaultId, keep);
      const parts = [`${updated} 篇已索引`];
      if (skipped > 0) parts.push(`${skipped} 篇未变更`);
      if (failed > 0) parts.push(`${failed} 篇失败`);
      if (pruned > 0) parts.push(`清理 ${pruned} 条失效记录`);
      this.showToast(`索引完成：${parts.join('，')}`);
    } catch (e) {
      this.showError(e);
    } finally {
      this.indexing = false;
      this.indexProgress = null;
    }
  }
}

export const vault = new Vault();
