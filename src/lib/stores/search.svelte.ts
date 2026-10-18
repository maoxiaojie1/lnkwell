/**
 * 跨文件搜索状态（面板关闭后仍保留，便于"返回结果"）。
 *
 * 为什么放到 store 而不是组件内 state：
 * 点开一条结果会把搜索面板关掉；若状态在组件里，查询词与结果随之销毁，
 * 用户想回列表只能重新打开、重新输入。
 */
import { searchDocs, upsertVault, type SearchHit } from '../db';
import { vault } from './vault.svelte';

class Search {
  query = $state('');
  results = $state<SearchHit[]>([]);
  searched = $state(false);
  searching = $state(false);
  /** 键盘高亮的行（↑↓ 移动，Enter 打开） */
  activeIndex = $state(0);
  /** 上一次打开的条目：回到结果列表时给个标记 */
  lastOpenedEntry = $state<string | null>(null);

  /** <3 字符走 LIKE 兜底，界面上给一句说明 */
  get fuzzy() {
    const n = this.query.trim().length;
    return n > 0 && n < 3;
  }

  clear(): void {
    this.query = '';
    this.results = [];
    this.searched = false;
    this.activeIndex = 0;
  }

  move(delta: number): void {
    const n = this.results.length;
    if (n === 0) return;
    this.activeIndex = (this.activeIndex + delta + n) % n;
  }

  async run(): Promise<void> {
    const q = this.query.trim();
    if (!q || !vault.root) {
      this.results = [];
      this.searched = false;
      return;
    }
    this.searching = true;
    try {
      const v = await upsertVault(vault.name, vault.root);
      if (!v) return;
      this.results = await searchDocs(v.id, q);
      this.searched = true;
      this.activeIndex = 0;
    } finally {
      this.searching = false;
    }
  }
}

export const search = new Search();
