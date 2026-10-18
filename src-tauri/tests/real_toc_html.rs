//! 验证大文档的 `[TOC]` 嵌套结构（改进计划 C3：改用合成文档）。

mod common;

use lnkwell_lib::markdown::render_document;

#[test]
fn large_doc_toc_is_nested_and_closed() {
    let md = common::synthetic_toc_doc(80);
    let r = render_document(&md);
    let html = r.chunks.join("\n");

    assert!(html.contains("<ul class=\"md-toc-list\">"), "应有嵌套 ul");
    assert!(html.contains("</ul></nav>"), "nav 应正常闭合");
    assert!(html.contains("md-toc://"), "链接应带哨兵");

    let li_count = html.matches("class=\"toc-item").count();
    assert!(li_count >= 80, "目录项应 ≥80，实际 {li_count}");

    // 每个目录项都必须是一个可点击的锚点链接
    let link_count = html.matches("<a href=\"md-toc://").count();
    assert_eq!(link_count, li_count, "目录项与链接数量应一一对应");
}
