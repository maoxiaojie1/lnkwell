/**
 * 阅读位置状态：桌面顶栏的进度线 + 右侧大纲的当前章节高亮。
 *
 * 与「阅读进度持久化」分开：
 * - 这里每次滚动都可能更新，属于瞬时 UI 状态；
 * - 写库走 ArticleView 的 800ms 防抖（且需要 vaultId）。
 */

class Reading {
  /** 0..1，当前文档滚动进度 */
  progress = $state(0);
  /** 当前视口顶部最近的标题 id（对应 TocItem.id） */
  anchor = $state<string | null>(null);

  reset(): void {
    this.progress = 0;
    this.anchor = null;
  }
}

export const reading = new Reading();
