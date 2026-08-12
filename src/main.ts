import { mount } from 'svelte';
import App from './App.svelte';

// 主题契约与正文样式（方案 §6）
import './themes/_contract.css';
import './themes/github-light.css';
import './themes/github-dark.css';
import './themes/sepia.css';
import './themes/markdown-body.css';
import './themes/app.css';

// KaTeX 样式（npm 引入，Vite 会连同 fonts 一起打包，保证离线可用，方案 §5.3）
import 'katex/dist/katex.min.css';

// 挂载前移除 #boot 启动层：它是 position:fixed 全屏覆盖层，
// 不移除会挡住下面所有按钮的点击（移动端尤为明显）。
document.getElementById('boot')?.remove();

const app = mount(App, { target: document.getElementById('app')! });

export default app;
