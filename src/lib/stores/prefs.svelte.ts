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
  #lineHeight = $state<number | null>(null);
  #measure = $state('comfortable');
  #railWidth = $state(258);
  #outlineWidth = $state(288);

  /** 桌面两端栏的宽度范围（px） */
  static readonly RAIL_MIN = 200;
  static readonly RAIL_MAX = 420;
  static readonly OUTLINE_MIN = 220;
  static readonly OUTLINE_MAX = 460;

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
  /**
   * 行高：null = 跟随主题（三套主题各自的行高不同）。
   * 之前这里总是写内联变量，把主题里的 --md-line-height 完全盖掉了。
   */
  set lineHeight(v: number | null) {
    this.#lineHeight = v;
    if (v == null) document.documentElement.style.removeProperty('--md-line-height');
    else document.documentElement.style.setProperty('--md-line-height', String(v));
    void this.#persist();
  }

  /** 实际生效的行高（未手动设置时取主题默认值），给设置面板的滑块用 */
  effectiveLineHeight(): number {
    if (this.#lineHeight != null) return this.#lineHeight;
    const raw = getComputedStyle(document.documentElement)
      .getPropertyValue('--md-line-height')
      .trim();
    return Number.parseFloat(raw) || 1.82;
  }

  get measure() { return this.#measure; }
  set measure(v: string) {
    this.#measure = v;
    document.documentElement.dataset.measure = v;
    void this.#persist();
  }

  /**
   * 左栏 / 右栏宽度：拖拽时每帧都会 set，所以这里**只改样式不落盘**，
   * 由调用方在拖拽结束时调 persistLayout()，避免拖一次写几十次配置文件。
   */
  get railWidth() { return this.#railWidth; }
  set railWidth(v: number) {
    const max = Math.min(Prefs.RAIL_MAX, Math.max(Prefs.RAIL_MIN, window.innerWidth - 560));
    this.#railWidth = Math.round(Math.min(max, Math.max(Prefs.RAIL_MIN, v)));
    document.documentElement.style.setProperty('--rail-w', `${this.#railWidth}px`);
  }

  get outlineWidth() { return this.#outlineWidth; }
  set outlineWidth(v: number) {
    const max = Math.min(Prefs.OUTLINE_MAX, Math.max(Prefs.OUTLINE_MIN, window.innerWidth - 560));
    this.#outlineWidth = Math.round(Math.min(max, Math.max(Prefs.OUTLINE_MIN, v)));
    document.documentElement.style.setProperty('--outline-w', `${this.#outlineWidth}px`);
  }

  /** 拖拽结束 / 双击复位时调用 */
  persistLayout() {
    void this.#persist();
  }

  /** 应用启动时调用一次，从磁盘恢复 */
  async hydrate() {
    try {
      const saved = await api.loadPrefs();
      this.theme = saved.theme;
      this.fontSize = saved.fontSize;
      // 1.7 是旧版本的写死默认值（不是用户选的），迁移成"跟随主题"
      const OLD_DEFAULT_LINE_HEIGHT = 1.7;
      this.lineHeight =
        saved.lineHeight == null || saved.lineHeight === OLD_DEFAULT_LINE_HEIGHT
          ? null
          : saved.lineHeight;
      if (saved.measure) this.measure = saved.measure;
      else document.documentElement.dataset.measure = this.#measure;
      if (saved.railWidth) this.railWidth = saved.railWidth;
      if (saved.outlineWidth) this.outlineWidth = saved.outlineWidth;
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
        railWidth: this.#railWidth,
        outlineWidth: this.#outlineWidth,
      })
      .catch(() => {});
  }
}

export const prefs = new Prefs();
