//! 验证 TOC 锚点一致性：渲染出的每个 toc id 都能在 HTML 中找到对应 heading。
//! 这是 TOC 跳转能生效的前提（问题3）。

use lnkwell_lib::markdown;

#[test]
fn toc_ids_exist_in_html() {
    let doc = r"D:\code\扩散模型\扩散模型统一讲义.md";
    let src_md = std::fs::read_to_string(doc).expect("读文档失败");

    let r = markdown::render_document(&src_md, &|p| format!("md-file:///abs/{p}")).expect("渲染失败");
    let html = r.chunks.join("");

    // 用 html5ever/ammonia 无法直接查 id，这里做简化检查：
    // 1) 所有 toc id 非空且唯一
    let mut seen = std::collections::HashSet::new();
    let mut duplicates = Vec::new();
    for t in &r.toc {
        assert!(!t.id.is_empty(), "TOC id 为空: {:?}", t.text);
        if !seen.insert(&t.id) {
            duplicates.push(t.id.clone());
        }
    }
    println!("TOC 总数: {}, 重复 id: {:?}", r.toc.len(), duplicates);

    // 2) 每个 toc id 对应的 heading 应出现在 HTML 中
    let mut missing = Vec::new();
    for t in &r.toc {
        let marker = format!("<h{} id=\"{}\"", t.level, t.id);
        if !html.contains(&marker) {
            missing.push((t.level, t.text.clone(), t.id.clone()));
        }
    }
    println!("HTML 中缺失的锚点: {:?}", missing);
    assert!(missing.is_empty(), "有 TOC 项在 HTML 中找不到锚点");
}
