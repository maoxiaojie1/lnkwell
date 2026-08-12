/**
 * TOC 跳转懒加载协调（问题：长文档目标章节可能还没渲染进 DOM）。
 *
 * TocDrawer 只负责记录目标锚点；ArticleView 的 $effect 监听该锚点，
 * 若 DOM 中不存在则继续追加分块，直到目标渲染出来后平滑滚动到位。
 * 两者通过本模块的 $state 解耦，避免组件层级的层层回调传递。
 */
class TocJump {
  /** 待跳转的锚点 id；null 表示无待处理跳转 */
  #target = $state<string | null>(null);

  get target() {
    return this.#target;
  }

  /** 请求跳转到指定锚点（未渲染的章节由 ArticleView 负责渲染后再滚动） */
  request(id: string) {
    this.#target = id;
  }

  /** 清除待处理跳转（渲染/滚动完成后调用） */
  clear() {
    this.#target = null;
  }
}

export const tocJump = new TocJump();
