# 更新日志

格式遵循 [Keep a Changelog](https://keepachangelog.com/zh-CN/1.1.0/)，版本号遵循 [语义化版本](https://semver.org/lang/zh-CN/)。

## [Unreleased]

## [0.1.0] - 2026-11-11

首个公开版本。

### 新增

**渲染**
- Rust 侧用 `pulldown-cmark` 解析，`ammonia` 白名单消毒后注入 WebView
- 代码高亮（highlight.js）、LaTeX 公式（KaTeX）、Mermaid 图表（懒加载）
- GitHub 风格 Alert（`[!NOTE]` / `[!TIP]` / `[!IMPORTANT]` / `[!WARNING]` / `[!CAUTION]`）
- 内嵌 `<svg>` 源码直接渲染（去脚本、去事件属性），进图和导出都生效
- 相对路径图片经 `md-rel://` 哨兵由后端解析：桌面走 `std::path`，Android 走 SAF
- 按顶层 H2 分块 + `IntersectionObserver` 懒注入，长文档首屏更快

**阅读**
- 右侧大纲：桌面端为可拖拽宽度的同层侧栏，窄屏退化为全屏抽屉；文内 `[TOC]` 与大纲互斥显示
- 阅读进度、段落级书签、多文档库切换与最近库列表
- 字号调节与页面缩放（`Ctrl 滚轮` / `Ctrl ±` / `Ctrl 0`），缩放写入 `--md-zoom` 与图片一起变化
- 只读源码视图、并排对照、图片查看器、拖拽打开文件或文件夹
- 导出 PDF（Windows，WebView2 `PrintToPdf`，无页眉页脚）

**检索**
- SQLite FTS5 `trigram` 全文搜索，跨文件定位到块
- 文档内查找（`Ctrl F` / `F3`），命中计数与文内高亮

**平台**
- Android：SAF 文件访问、`WindowInsets` 安全区注入、物理返回键接管、分享/深链打开
- Windows：WebView2、单实例转发、双击 `.md` 关联启动
- Linux / macOS：可从源码构建，未提供预编译包

### 修复

- **Android 上文档内的图片（尤其 SVG）无法显示**：`EntryRef` 在 Android 是 `content://` 的
  JSON 串、不含扩展名，data URL 的 MIME 退化为 `application/octet-stream`，Chrome 拒绝渲染；
  SVG 又不像 PNG 能靠内容嗅探救回来，于是只有 SVG 全挂。改为按文件魔数识别 MIME
  （png / jpeg / gif / webp / bmp / ico / svg），扩展名仅作兜底。

### 已知限制

- 单文件上限 10MB（图片 32MB）
- 全文搜索查询词至少 3 个字符（trigram 分词）
- Android SAF 无稳定 mtime，索引每次启动重建
- PDF 导出仅 Windows

[Unreleased]: https://github.com/maoxiaojie1/lnkwell/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/maoxiaojie1/lnkwell/releases/tag/v0.1.0
