//! 用用户真实文档验证 [TOC] 标记渲染

use lnkwell_lib::markdown;

#[test]
fn real_doc_toc_marker() {
    let doc = r"D:\code\扩散模型\扩散模型统一讲义.md";
    let src_md = std::fs::read_to_string(doc).expect("读文档失败");
    assert!(src_md.contains("[TOC]"), "文档应有 [TOC] 标记");

    let r = markdown::render_document(&src_md, &|p| format!("md-file:///abs/{p}")).expect("渲染失败");
    let html = r.chunks.join("\n");

    println!("==== 真实文档 [TOC] 渲染 ====");
    // 逐行打印含 md-toc 的行（避免字节切片越界）
    for line in html.lines() {
        if line.contains("md-toc") {
            println!("{line}");
        }
    }
    println!("==== 结束 ====");

    assert!(html.contains("md-toc"), "[TOC] 应渲染为目录");
    assert!(!html.contains("[TOC]"), "[TOC] 原文应被替换");
    // 目录里的链接应可跳转（md-toc:// 哨兵）
    assert!(html.contains("md-toc://"), "目录链接应带哨兵 scheme");
}
