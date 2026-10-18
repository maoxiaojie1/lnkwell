//! fixtures 回归测试（改进计划 C3）。
//!
//! 目标不是逐字节快照，而是锁住渲染管线的**不变量**：
//! 消毒、分块平衡、TOC 指向、编码回退、围栏内不被改写、路径哨兵、确定性。
//! 改动 `markdown.rs` 之后应先跑这一套。

use lnkwell_lib::markdown::{
    decode_text, extract_index, normalize_newlines, render_document, RenderResult,
};
use std::path::{Path, PathBuf};

fn case_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("fixtures")
        .join("cases")
        .join(name)
}

fn load(name: &str) -> RenderResult {
    let bytes =
        std::fs::read(case_path(name)).unwrap_or_else(|e| panic!("读取 fixture {name} 失败: {e}"));
    let (text, _enc) = decode_text(&bytes).unwrap_or_else(|e| panic!("解码 {name} 失败: {e}"));
    render_document(&normalize_newlines(&text))
}

const ALL_CASES: &[&str] = &[
    "basic.md",
    "crlf.md",
    "gbk.md",
    "xss.md",
    "toc-in-fence.md",
    "blockquote-h2.md",
    "nested.md",
    "edge-paths.md",
];

#[test]
fn every_fixture_is_sanitized_and_consistent() {
    for &name in ALL_CASES {
        let r = load(name);
        let html = r.chunks.concat();
        assert!(!html.is_empty(), "{name}: 渲染结果为空");
        assert!(!html.contains("<script"), "{name}: 残留 <script>");
        assert!(!html.contains("<iframe"), "{name}: 残留 <iframe>");
        assert!(!html.contains("onerror="), "{name}: 残留事件属性");
        assert!(
            !html.to_lowercase().contains("javascript:"),
            "{name}: 残留 javascript: URL"
        );
        // TOC 的 chunk 序号必须指向真正包含该标题的块
        for item in &r.toc {
            let chunk = r
                .chunks
                .get(item.chunk)
                .unwrap_or_else(|| panic!("{name}: TOC「{}」chunk 越界", item.text));
            assert!(
                chunk.contains(&format!("id=\"{}\"", item.id)),
                "{name}: TOC「{}」指向的块不含该标题",
                item.text
            );
        }
    }
}

#[test]
fn gbk_fixture_decodes_to_chinese() {
    let bytes = std::fs::read(case_path("gbk.md")).expect("读取 gbk.md");
    let (text, enc) = decode_text(&bytes).expect("GBK 应能解码");
    assert_eq!(enc, "GB18030");
    assert!(text.contains("环境配置说明"), "实际: {text}");
}

#[test]
fn crlf_fixture_is_normalized() {
    let bytes = std::fs::read(case_path("crlf.md")).expect("读取 crlf.md");
    assert!(
        bytes.windows(2).any(|w| w == b"\r\n"),
        "fixture 本身应含 CRLF"
    );
    let (text, _enc) = decode_text(&bytes).expect("解码");
    let normalized = normalize_newlines(&text);
    assert!(!normalized.contains('\r'), "归一化后不应残留 CR");
    let r = render_document(&normalized);
    assert!(!r.chunks.concat().contains('\r'));
}

#[test]
fn toc_marker_inside_fence_is_preserved() {
    let html = load("toc-in-fence.md").chunks.concat();
    assert!(
        html.contains("[TOC]"),
        "围栏代码块里的 [TOC] 必须原样保留: {html}"
    );
    assert!(
        html.contains("md-toc://"),
        "围栏外的 [TOC] 仍应生成真实目录: {html}"
    );
}

#[test]
fn blockquote_heading_does_not_break_containers() {
    let r = load("blockquote-h2.md");
    for (i, chunk) in r.chunks.iter().enumerate() {
        let opens = chunk.matches("<blockquote>").count();
        let closes = chunk.matches("</blockquote>").count();
        assert_eq!(opens, closes, "第 {i} 块 blockquote 不平衡：{chunk}");
        let ul_open = chunk.matches("<ul").count();
        let ul_close = chunk.matches("</ul>").count();
        assert_eq!(ul_open, ul_close, "第 {i} 块 <ul> 不平衡：{chunk}");
    }
}

#[test]
fn relative_paths_become_platform_free_sentinels() {
    let html = load("edge-paths.md").chunks.concat();
    // 渲染期不得出现平台相关路径（那是 A1 要消除的东西）
    assert!(!html.contains("md-file://"), "不应再产出 md-file:// 哨兵");
    assert!(!html.contains("C:\\"), "不应出现 Windows 绝对路径");
    // 相对图片 → md-rel://
    assert!(html.contains("md-rel://./img/a.png"), "实际: {html}");
    assert!(html.contains("md-rel://../assets/x.png"), "实际: {html}");
    // 互跳 → md-link://
    assert!(html.contains("md-link://./sibling.md"), "实际: {html}");
    // 外链与 data URL 原样保留
    assert!(html.contains("https://example.com/a.png"));
    assert!(html.contains("data:image/png;base64"));
}

#[test]
fn rendering_is_deterministic() {
    for &name in ALL_CASES {
        let a = load(name);
        let b = load(name);
        assert_eq!(a.chunks, b.chunks, "{name}: 两次渲染 HTML 不一致");
        assert_eq!(a.toc.len(), b.toc.len(), "{name}: 两次渲染 TOC 长度不一致");
        let ids_a: Vec<&str> = a.toc.iter().map(|t| t.id.as_str()).collect();
        let ids_b: Vec<&str> = b.toc.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids_a, ids_b, "{name}: 标题 id 不稳定");
    }
}

/// 长文档：H2 数量与分块数量线性对应，chunk 序号单调且不越界
#[test]
fn long_document_chunks_linearly() {
    const N: usize = 400;
    let mut md = String::from("# 长文\n\n导言段落。\n\n");
    for i in 0..N {
        md.push_str(&format!("## 第 {i} 节\n\n正文 {i}。\n\n"));
    }
    let r = render_document(&md);
    assert_eq!(r.chunks.len(), N + 1, "每节一块 + 导言块");
    assert_eq!(r.toc.len(), N + 1, "H1 + N 个 H2");
    for w in r.toc.windows(2) {
        assert!(w[0].chunk <= w[1].chunk, "chunk 序号应单调不减");
    }
    for item in &r.toc {
        assert!(item.chunk < r.chunks.len(), "chunk 越界");
    }
}

#[test]
fn index_extraction_prefers_frontmatter_title() {
    let bytes = std::fs::read(case_path("basic.md")).expect("读取 basic.md");
    let (text, _enc) = decode_text(&bytes).expect("解码");
    let doc = extract_index(&normalize_newlines(&text));
    assert_eq!(doc.title.as_deref(), Some("基础样本"));
    assert!(doc.plain.contains("任务列表"), "纯文本应含正文");
    assert!(!doc.plain.contains("|---"), "表格分隔行不应进索引");
    assert!(!doc.plain.contains("title:"), "front matter 不应进索引");
}
