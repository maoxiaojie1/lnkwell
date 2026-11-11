//! 验证 `[TOC]` 标记渲染为文档内目录（嵌套列表结构）

use lnkwell_lib::markdown::render_document;

fn render(md: &str) -> String {
    render_document(md).chunks.join("\n")
}

#[test]
fn toc_marker_renders_nested_list() {
    let md = "# 标题一\n\n[TOC]\n\n## 二级A\n\n## 二级B\n\n### 三级C";
    let html = render(md);

    // 目录 nav 存在
    assert!(html.contains("md-toc"), "应有文档内目录");
    // 链接用 md-toc:// 哨兵
    assert!(html.contains("md-toc://"), "目录链接应使用 md-toc:// 哨兵");
    // 嵌套列表结构：三级标题应有独立的子 <ul>
    assert!(html.contains("<ul class=\"md-toc-list\">"), "应有嵌套 ul");
    let ul_count = html.matches("<ul class=\"md-toc-list\">").count();
    assert!(ul_count >= 2, "应有外层与子级两个 ul，实际 {ul_count}");
    // li 包裹标题（id 经 slugify 转小写 ASCII），且带层级 class
    assert!(
        html.contains("<li class=\"toc-item toc-lvl-1\">"),
        "H1 应带 toc-lvl-1"
    );
    assert!(
        html.contains("<li class=\"toc-item toc-lvl-2\"><a href=\"md-toc://二级a\">二级A</a></li>"),
        "二级A 应带 toc-lvl-2"
    );
    assert!(
        html.contains("<li class=\"toc-item toc-lvl-3\">"),
        "三级C 应带 toc-lvl-3"
    );
    // 正文标题保留 id
    assert!(html.contains("<h2 id=\"二级a\""), "正文标题应保留 id");
    // 不再出现 [TOC] 原文
    assert!(!html.contains("[TOC]"), "[TOC] 原文应被替换");
}

#[test]
fn toc_marker_case_insensitive() {
    let html = render("# 标题\n\n[toc]\n\n## 小节");
    assert!(html.contains("md-toc"), "小写 [toc] 也应识别");
}

#[test]
fn toc_marker_no_toc_empty() {
    // 无标题时 [TOC] 应退化为空（不产生空 nav 干扰）
    let html = render("[TOC]\n\n普通文本");
    assert!(!html.contains("md-toc"), "无标题时不应生成目录");
}

#[test]
fn toc_nesting_flat_and_deep() {
    // 混合层级：H1 → H2 → H3 → H2，验证树的关层正确
    let html = render("# A\n\n[TOC]\n\n## B\n\n### C\n\n## D\n\n#### E");

    let b_pos = html.find("md-toc://b").expect("B 在目录");
    let c_pos = html.find("md-toc://c").expect("C 在目录");
    let d_pos = html.find("md-toc://d").expect("D 在目录");
    let e_pos = html.find("md-toc://e").expect("E 在目录");
    assert!(
        b_pos < c_pos && c_pos < d_pos && d_pos < e_pos,
        "目录顺序应保持文档顺序"
    );

    // 树结构正确性：C 的 <li> 前面应有子 <ul>（属于 B 的嵌套）
    let before_c = &html[b_pos..c_pos];
    assert!(before_c.contains("</a><ul"), "B 的 li 应包含子 ul");
    // E 同理
    let before_e = &html[d_pos..e_pos];
    assert!(before_e.contains("</a><ul"), "D 的 li 应包含子 ul");
}
