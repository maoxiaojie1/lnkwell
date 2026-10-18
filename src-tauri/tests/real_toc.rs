//! 验证 `[TOC]` 在大文档下的渲染（改进计划 C3：改用合成文档，不再依赖本机绝对路径）。

mod common;

use lnkwell_lib::markdown::render_document;

#[test]
fn large_doc_toc_marker() {
    let md = common::synthetic_toc_doc(80);
    let r = render_document(&md);
    let html = r.chunks.join("\n");

    assert!(html.contains("md-toc"), "[TOC] 应渲染为目录");
    assert!(!html.contains("[TOC]"), "[TOC] 原文应被替换");
    // 目录里的链接应可跳转（md-toc:// 哨兵）
    assert!(html.contains("md-toc://"), "目录链接应带哨兵 scheme");
    // 80 节 + 导言 + H1
    let items = html.matches("class=\"toc-item").count();
    assert!(items >= 80, "目录项应 ≥80，实际 {items}");
}
