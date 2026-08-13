//! 验证 TOC 的 chunk 序号正确性：每个标题的 chunk 值必须落在合法范围，
//! 且任意标题渲染到它的 chunk 后 DOM 中必然能找到锚点（一步跳转的前提）。

use lnkwell_lib::markdown;

#[test]
fn toc_chunks_valid_and_in_bounds() {
    let doc = r"D:\code\扩散模型\扩散模型统一讲义.md";
    let src_md = std::fs::read_to_string(doc).expect("读文档失败");

    let r = markdown::render_document(&src_md, &|p| format!("md-file:///abs/{p}")).expect("渲染失败");

    println!("chunks 总数: {}", r.chunks.len());
    println!("toc 总数: {}", r.toc.len());

    let mut prev_chunk = 0usize;
    let mut bad = Vec::new();
    for t in &r.toc {
        if t.chunk >= r.chunks.len() {
            bad.push(format!("越界: {} chunk={} (总 {})", t.text, t.chunk, r.chunks.len()));
        }
        // chunk 号非递减（H2 边界内标题共享块；跨块必须递增）
        if t.chunk < prev_chunk {
            bad.push(format!("回退: {} chunk={} < 前一个 {}", t.text, t.chunk, prev_chunk));
        }
        prev_chunk = t.chunk;

        // 关键断言：渲染到 t.chunk 后，该块 HTML 应包含目标锚点
        let marker = format!("<h{} id=\"{}\"", t.level, t.id);
        if !r.chunks[t.chunk].contains(&marker) {
            // H1 可能在首个 H2 块里（标题在文档顶部），放宽：检查 chunk 0..=t.chunk 前缀
            let found = r.chunks[..=t.chunk].iter().any(|c| c.contains(&marker));
            if !found {
                bad.push(format!("找不到锚点: {} id={} chunk={}", t.text, t.id, t.chunk));
            }
        }
    }

    println!("问题项: {:?}", bad);
    assert!(bad.is_empty(), "TOC chunk 标注错误: {bad:?}");
}
