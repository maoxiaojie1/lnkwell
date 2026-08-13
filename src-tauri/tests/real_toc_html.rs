//! 用真实文档验证 [TOC] 渲染为嵌套列表

use lnkwell_lib::markdown;

#[test]
fn real_doc_toc_nested() {
    let doc = r"D:\code\扩散模型\扩散模型统一讲义.md";
    let src_md = std::fs::read_to_string(doc).expect("读文档失败");
    let r = markdown::render_document(&src_md, &|p| format!("md-file:///abs/{p}")).expect("渲染失败");
    let html = r.chunks.join("\n");

    // 输出目录结构（逐行打印，避免字节切片越界）
    println!("==== TOC 片段 ====");
    for line in html.lines() {
        if line.contains("md-toc") {
            println!("{line}");
        }
    }
    println!("==== 结束 ====");

    assert!(html.contains("<ul class=\"md-toc-list\">"), "应有嵌套 ul");
    assert!(html.contains("</ul></nav>"), "nav 应正常闭合");
    assert!(html.contains("md-toc://"), "链接应带哨兵");
    // 检查 ul 数量与 li 数量合理性（至少 60 项目录）
    let li_count = html.matches("<li>").count();
    println!("目录项数量: {li_count}");
    assert!(li_count >= 60, "真实讲义目录应 ≥60 项，实际 {li_count}");
}
