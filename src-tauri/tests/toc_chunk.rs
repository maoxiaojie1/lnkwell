//! 验证 TOC 的 chunk 序号正确性：每个标题的 chunk 值必须落在合法范围，
//! 且任意标题渲染到它的 chunk 后 DOM 中必然能找到锚点（一步跳转的前提）。
//!
//! 改进计划 C3：改用合成文档，不再依赖机器本地路径。

mod common;

use lnkwell_lib::markdown::{render_document, RenderResult};

fn check(label: &str, r: &RenderResult) {
    let mut bad = Vec::new();
    let mut prev_chunk = 0usize;

    for t in &r.toc {
        if t.chunk >= r.chunks.len() {
            bad.push(format!("越界: {} chunk={} (总 {})", t.text, t.chunk, r.chunks.len()));
        }
        // chunk 号非递减（同一块内多个标题共享块号；跨块必须递增）
        if t.chunk < prev_chunk {
            bad.push(format!("回退: {} chunk={} < 前一个 {}", t.text, t.chunk, prev_chunk));
        }
        prev_chunk = t.chunk;

        // 关键断言：渲染到 t.chunk 后，该块 HTML 应包含目标锚点。
        // 允许标题落在更早的块里（同一块内的后续标题），故检查 0..=chunk 前缀。
        let marker = format!("<h{} id=\"{}\"", t.level, t.id);
        if t.chunk < r.chunks.len() {
            let found = r.chunks[..=t.chunk].iter().any(|c| c.contains(&marker));
            if !found {
                bad.push(format!("找不到锚点: {} id={} chunk={}", t.text, t.id, t.chunk));
            }
        }
    }

    assert!(bad.is_empty(), "{label}: TOC chunk 标注错误: {bad:?}");
}

#[test]
fn toc_chunks_valid_in_synthetic_doc() {
    let md = common::synthetic_toc_doc(80);
    check("synthetic-80", &render_document(&md));
}

#[test]
fn toc_chunks_valid_in_fixtures() {
    for name in ["basic.md", "nested.md", "blockquote-h2.md"] {
        check(name, &common::render_case(name));
    }
}
