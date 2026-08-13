/**
 * 导出 PDF 的样式（方案 §7.2）。
 *
 * 为什么不用 @media print：WebView2 的 PrintToPdf 是对屏幕内容的直接快照，
 * 未必应用 print 媒体查询——这会导致 @media print 里的白底/隐藏 UI 失效，
 * 深色主题下 sidebar、正文四周留白原样进 PDF，形成"大黑边"。
 * 故改为导出前用 JS 注入一段不带媒体包装的内联样式（全部 !important），
 * 强制白底深字 + 隐藏 UI 壳层 + 技术文档排版，导出后再移除。
 *
 * 样式风格参考 md2pdf_watermark.py：Times 衬线正文 12pt / 行距 1.8 /
 * 浅灰代码块 / 蓝色引用卡片 / 全边框表格 / 代码自动换行。
 */

const EXPORT_STYLE = `
  /* 主题变量重置：无论屏幕深浅主题，导出一律白底深字 */
  :root, html[data-theme] {
    --md-bg: #ffffff !important;
    --md-fg: #333333 !important;
    --md-fg-muted: #59636e !important;
    --md-accent: #4a90d9 !important;
    --md-border: #999999 !important;
    --md-code-bg: #f5f5f5 !important;
    --md-inline-code-bg: #f5f5f5 !important;
    --md-quote-bar: #4a90d9 !important;
    --md-quote-fg: #444444 !important;
    --md-table-head: #f2f2f2 !important;
    --md-table-stripe: #ffffff !important;
    --hljs-keyword: #cf222e !important;
    --hljs-string: #0a3069 !important;
    --hljs-comment: #59636e !important;
    --hljs-number: #0550ae !important;
    --hljs-function: #8250df !important;
    --hljs-type: #953800 !important;
  }

  /* 页面/容器底色强制白，消除深色主题下 body 背景露出的黑框 */
  html, body, #app, .layout, .main-area, .reader-scroll, .markdown-body {
    background: #ffffff !important;
    box-shadow: none !important;
  }

  /* 隐藏所有 UI 壳层，只留正文 */
  .toolbar, .sidebar, .drawer, .drawer-backdrop, .sidebar-backdrop,
  .find-bar, .welcome, .toast {
    display: none !important;
  }
  #app {
    padding-top: 0 !important;
    height: auto !important;
    overflow: visible !important;
  }
  .layout { display: block !important; }
  .main-area { display: block !important; }
  .reader-scroll {
    overflow: visible !important;
    height: auto !important;
  }

  /* 正文基底：Times 衬线 12pt、行距 1.8、深灰字 */
  .markdown-body {
    background: #ffffff !important;
    color: #333333 !important;
    font-family: "Times New Roman", Georgia, "SimSun", "Source Han Serif SC",
      "Noto Serif CJK SC", serif !important;
    font-size: 12pt !important;
    line-height: 1.8 !important;
    max-width: 100% !important;
    padding: 0 !important;
    margin: 0 !important;
    text-align: justify !important;
  }

  /* front matter：简洁标题区，居中，紧接正文 */
  .markdown-body .frontmatter-card {
    border: none !important;
    background: transparent !important;
    padding: 0 0 1em !important;
    margin: 0 0 1.4em !important;
    text-align: center !important;
    border-bottom: 1px solid #ddd !important;
  }
  .markdown-body .fm-title {
    font-family: "Segoe UI", "Microsoft YaHei", "Source Han Sans SC",
      "Noto Sans CJK SC", sans-serif !important;
    font-size: 20pt !important;
    font-weight: 700 !important;
    text-align: center !important;
    color: #1f1f1f !important;
  }
  .markdown-body .fm-date {
    text-align: center !important;
    color: #666 !important;
    font-size: 9.5pt !important;
    margin-top: 0.4em !important;
  }
  .markdown-body .fm-tags { justify-content: center !important; margin-top: 0.5em !important; }
  .markdown-body .fm-tag {
    background: transparent !important;
    color: #555 !important;
    border: 1px solid #bbb !important;
    font-size: 9pt !important;
  }

  /* 标题层级：无衬线、左对齐、连续排版 */
  .markdown-body h1, .markdown-body h2, .markdown-body h3,
  .markdown-body h4, .markdown-body h5, .markdown-body h6 {
    font-family: "Segoe UI", "Microsoft YaHei", "Source Han Sans SC",
      "Noto Sans CJK SC", sans-serif !important;
    color: #1f1f1f !important;
    border: none !important;
    text-align: left !important;
    page-break-after: avoid;
  }
  .markdown-body h1 { font-size: 20pt !important; font-weight: 700 !important; margin: 1.4em 0 0.7em !important; padding: 0 !important; }
  .markdown-body h2 { font-size: 15pt !important; font-weight: 700 !important; margin: 1.2em 0 0.5em !important; padding: 0 !important; }
  .markdown-body h3 { font-size: 13pt !important; font-weight: 600 !important; margin: 1em 0 0.4em !important; }
  .markdown-body h4 { font-size: 12pt !important; font-weight: 600 !important; }

  /* 正文段落：无缩进、两端对齐 */
  .markdown-body p {
    text-indent: 0 !important;
    text-align: justify !important;
    margin: 0 0 0.6em !important;
    orphans: 2;
    widows: 2;
  }

  /* 链接：深蓝字、去下划线 */
  .markdown-body a {
    color: #1a6bb0 !important;
    border: none !important;
    text-decoration: none !important;
  }

  /* 引用块：浅蓝底卡片 + 蓝左边框 */
  .markdown-body blockquote {
    background: #f7f9fc !important;
    border-left: 4px solid #4a90d9 !important;
    border-radius: 6px !important;
    padding: 12px 16px !important;
    margin: 12px 0 !important;
    box-shadow: 0 2px 8px rgb(0 0 0 / 0.08) !important;
    color: #444 !important;
    page-break-inside: avoid;
  }

  /* 代码：Consolas + 浅灰底 + 圆角，代码块自动换行不溢出 */
  .markdown-body code {
    font-family: Consolas, "Courier New", monospace !important;
    font-size: 0.9em !important;
    background: #f5f5f5 !important;
    color: #1a1a1a !important;
    padding: 2px 4px !important;
    border-radius: 3px !important;
  }
  .markdown-body pre {
    background: #f5f5f5 !important;
    /* 不打印背景时，代码块靠边框区分（边框会打印） */
    border: 1px solid #d9d9d9 !important;
    border-radius: 5px !important;
    padding: 12px !important;
    box-shadow: none !important;
    white-space: pre-wrap !important;
    word-break: break-word !important;
    page-break-inside: avoid;
  }
  .markdown-body pre code { background: transparent !important; padding: 0 !important; }

  /* 表格：全边框 + 表头浅灰 */
  .markdown-body table {
    border-collapse: collapse !important;
    width: 100% !important;
    margin: 10px 0 !important;
    page-break-inside: avoid;
  }
  .markdown-body th, .markdown-body td {
    border: 1px solid #999 !important;
    padding: 6px 10px !important;
  }
  .markdown-body th {
    background: #f2f2f2 !important;
    font-family: "Segoe UI", "Microsoft YaHei", "Source Han Sans SC",
      "Noto Sans CJK SC", sans-serif !important;
    font-weight: 600 !important;
  }
  .markdown-body tr:nth-child(2n) { background: transparent !important; }

  /* 图片：限制宽度不溢出、去阴影圆角 */
  .markdown-body img {
    max-width: 100% !important;
    box-shadow: none !important;
    border-radius: 0 !important;
    page-break-inside: avoid;
  }

  /* 文档内目录：连续排版、去边框背景 */
  .markdown-body .md-toc { border: none !important; background: transparent !important; }
  .markdown-body .md-toc-title { color: #1f1f1f !important; border-bottom: 1px solid #ccc !important; }
  .markdown-body .md-toc-title::before { display: none !important; }
  .markdown-body .md-toc a { color: #333 !important; }
  .markdown-body .toc-lvl-1 > a { color: #1f1f1f !important; }

  /* 隐藏交互/屏幕专属元素 */
  .markdown-body .code-copy-btn { display: none !important; }
  .markdown-body mark.find-mark { background: transparent !important; outline: none !important; }
  .markdown-body hr { background: #ccc !important; max-width: 100% !important; }
`;

let injectedStyle: HTMLStyleElement | null = null;

/** 注入导出样式，返回清理函数（导出结束后调用以恢复屏幕样式） */
export function applyExportStyle(): () => void {
  if (injectedStyle) return () => {};
  const style = document.createElement('style');
  style.id = 'export-pdf-style';
  // 关键：media="print" 让这些样式只在打印媒体下生效，屏幕（screen）不受影响，
  // 避免导出过程中 UI 壳层被隐藏、正文撑满窗口。PrintToPdf 走打印渲染路径会应用该媒体。
  style.media = 'print';
  style.textContent = EXPORT_STYLE;
  document.head.appendChild(style);
  injectedStyle = style;
  return () => {
    style.remove();
    if (injectedStyle === style) injectedStyle = null;
  };
}
