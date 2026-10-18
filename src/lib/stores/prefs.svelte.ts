import { api } from '../api';
import { refreshMermaidTheme } from '../render/mermaid';

/**
 * 偏好设置（方案 §6.5）。
 * ⚠️ 模块顶层单例禁止在构造器里用 `$effect`（会抛 effect_orphan），
 * 副作用全部放 setter 里——用户点击那一刻就是副作用发生的时机。
 */
class Prefs {
  #theme = $state('github-light');
  #fontSize = $state(16);
  #lineHeight = $state(1.7);
  #measure = $state('comfortable');

  get theme() { return this.#theme; }
  set theme(v: string) {
    this.#theme = v;
    document.documentElement.dataset.theme = v;
    void this.#syncWindowTheme(v);
    void this.#persist();
    void refreshMermaidTheme();
  }

  /**
   * 让系统标题栏跟随主题（Windows 上深色主题配白色标题栏很割裂）。
   * 浏览器预览 / 权限缺失时静默忽略。
   */
  async #syncWindowTheme(theme: string) {
    try {
      const { getCurrentWindow } = await import('@tauri-apps/api/window');
      await getCurrentWindow().setTheme(theme === 'github-dark' ? 'dark' : 'light');
    } catch {
      /* 非 Tauri 环境或权限未授予 */
    }
  }

  get fontSize() { return this.#fontSize; }
  set fontSize(v: number) {
    this.#fontSize = v;
    document.documentElement.style.setProperty('--md-font-size', `${v}px`);
    void this.#persist();
  }

  get lineHeight() { return this.#lineHeight; }
  set lineHeight(v: number) {
    this.#lineHeight = v;
    document.documentElement.style.setProperty('--md-line-height', String(v));
    void this.#persist();
  }

  get measure() { return this.#measure; }
  set measure(v: string) {
    this.#measure = v;
    document.documentElement.dataset.measure = v;
    void this.#persist();
  }

  /** 应用启动时调用一次，从磁盘恢复 */
  async hydrate() {
    try {
      const saved = await api.loadPrefs();
      this.theme = saved.theme;
      this.fontSize = saved.fontSize;
      this.lineHeight = saved.lineHeight;
      if (saved.measure) this.measure = saved.measure;
      else document.documentElement.dataset.measure = this.#measure;
    } catch {
      // 首次启动无配置文件，用默认值
      document.documentElement.dataset.measure = this.#measure;
    }
  }

  #persist() {
    return api
      .savePrefs({
        theme: this.#theme,
        fontSize: this.#fontSize,
        lineHeight: this.#lineHeight,
        measure: this.#measure,
      })
      .catch(() => {});
  }
}

export const prefs = new Prefs();
