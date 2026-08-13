/**
 * 导出 PDF 协调（方案 §7.2）。SettingsPanel 请求导出；
 * ArticleView 监听 pending 变化后先渲染全文（含图片加载），再 window.print()。
 * 用计数器而非布尔，避免连续两次导出被合并。
 */
class ExportStore {
  #pending = $state(0);

  get pending() {
    return this.#pending;
  }

  request() {
    this.#pending++;
  }
}

export const exportRequest = new ExportStore();
