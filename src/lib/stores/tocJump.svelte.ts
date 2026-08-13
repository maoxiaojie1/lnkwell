/**
 * TOC 跳转协调（问题3：长文档目录跳转未渲染章节）。
 *
 * TocDrawer 记录目标锚点 id + 目标所在 chunk 序号（后端 TocItem.chunk 已计算）；
 * ArticleView 的 $effect 监听后直接 `renderedCount = chunk + 1` 一步渲染到位，
 * 再滚动到锚点。不再依赖逐块探测 DOM，长文档跨大跨度跳转稳定。
 */
export interface TocTarget {
  id: string;
  /** 目标标题所在 chunk 序号（-1 表示未知，回退到全部渲染后查找） */
  chunk: number;
}

class TocJump {
  #target = $state<TocTarget | null>(null);

  get target() {
    return this.#target;
  }

  request(id: string, chunk: number) {
    this.#target = { id, chunk };
  }

  clear() {
    this.#target = null;
  }
}

export const tocJump = new TocJump();
