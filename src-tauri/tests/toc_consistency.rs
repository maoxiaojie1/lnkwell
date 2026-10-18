//! 验证 TOC 锚点一致性：渲染出的每个 toc id 都能在 HTML 中找到对应 heading。
//! 这是 TOC 跳转能生效的前提。
//!
//! 改进计划 C3：不再依赖开发者机器上的绝对路径，改用仓库内 fixture + 合成文档。

mod common;

use lnkwell_lib::markdown::{render_document, RenderResult};

fn assert_consistent(label: &str, r: &RenderResult) {
    let html = r.chunks.join("");

    // 1) 所有 toc id 非空且唯一
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    for t in &r.toc {
        assert!(!t.id.is_empty(), "{label}: TOC id 为空: {:?}", t.text);
        if !seen.insert(t.id.clone()) {
            duplicates.push(t.id.clone());
        }
    }
    assert!(duplicates.is_empty(), "{label}: TOC id 重复: {duplicates:?}");

    // 2) 每个 toc id 对应的 heading 应出现在 HTML 中
    let mut missing = Vec::new();
    for t in &r.toc {
        let marker = format!("<h{} id=\"{}\"", t.level, t.id);
        if !html.contains(&marker) {
            missing.push((t.level, t.text.clone(), t.id.clone()));
        }
    }
    assert!(missing.is_empty(), "{label}: 有 TOC 项在 HTML 中找不到锚点: {missing:?}");
}

#[test]
fn toc_ids_exist_in_fixtures() {
    for name in ["basic.md", "nested.md", "blockquote-h2.md", "toc-in-fence.md"] {
        assert_consistent(name, &common::render_case(name));
    }
}

#[test]
fn toc_ids_exist_in_synthetic_doc() {
    let md = common::synthetic_toc_doc(80);
    assert_consistent("synthetic", &render_document(&md));
}
