import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { api } from '../api';
import type { RenderResult } from '../api';

export interface OpenDoc {
  entry: string;
  name: string;
  result: RenderResult;
  /** 待恢复的滚动锚点 */
  anchor?: string;
  /** 临时分享文档（不属于任何 vault，无法收藏/建索引） */
  isTemp?: boolean;
}

/**
 * 导航栈（方案 §7.1）：md 互跳时入栈，返回键逐级回退。
 * 安卓返回栈约定：抽屉打开→关抽屉；有导航历史→回上一篇；否则退出。
 */
class Nav {
  #stack = $state<OpenDoc[]>([]);
  #loading = $state(false);
  #error = $state<string | null>(null);

  get current(): OpenDoc | null {
    return this.#stack.length > 0 ? this.#stack[this.#stack.length - 1] : null;
  }
  get canGoBack() { return this.#stack.length > 1; }
  get loading() { return this.#loading; }
  get error() { return this.#error; }

  /** 打开文档。fromLink=true 表示由文档内链接触发（保留返回路径） */
  async open(entry: string, name: string, opts: { replace?: boolean } = {}) {
    this.#loading = true;
    this.#error = null;
    try {
      const result = await api.readDoc(entry);
      const doc: OpenDoc = { entry, name, result };
      if (opts.replace || this.#stack.length === 0) {
        this.#stack = [doc];
      } else {
        this.#stack = [...this.#stack, doc];
      }
    } catch (e) {
      this.#error = entry;
      throw e;
    } finally {
      this.#loading = false;
    }
  }

  /** 打开临时分享文档（方案 §7.4）：URI 是临时授权单文件，不走 vault 路径 */
  async openTemp(uri: string, name: string) {
    this.#loading = true;
    this.#error = null;
    try {
      const result = await api.readTempDoc(uri);
      this.#stack = [{ entry: uri, name, result, isTemp: true }];
    } catch (e) {
      this.#error = uri;
      throw e;
    } finally {
      this.#loading = false;
    }
  }

  /** 监听系统分享事件（方案 §7.4）。返回清理函数。 */
  async listenForShare(): Promise<UnlistenFn> {
    // 分享 URI 通过 deep-link 事件或自定义事件到达
    return listen<string>('open-shared-file', (ev) => {
      const uri = ev.payload;
      const name = uri.replace(/\\/g, '/').split('/').pop()?.split('%2F').pop() ?? '分享文档';
      void this.openTemp(uri, decodeURIComponent(name));
    });
  }

  /** 返回上一篇；返回 false 表示栈底（应由调用方决定退出或关抽屉） */
  back(): boolean {
    if (this.#stack.length > 1) {
      this.#stack = this.#stack.slice(0, -1);
      return true;
    }
    return false;
  }

  closeAll() {
    this.#stack = [];
  }
}

export const nav = new Nav();
