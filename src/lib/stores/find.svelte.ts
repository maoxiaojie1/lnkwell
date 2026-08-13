/**
 * 文档内查找状态（Ctrl+F）。App 工具栏按钮打开、Escape 关闭；
 * ArticleView 监听 open 状态渲染查找栏并执行高亮。跨组件解耦。
 */
class FindStore {
  #open = $state(false);

  get open() {
    return this.#open;
  }

  show() {
    this.#open = true;
  }

  close() {
    this.#open = false;
  }

  toggle() {
    this.#open = !this.#open;
  }
}

export const find = new FindStore();
