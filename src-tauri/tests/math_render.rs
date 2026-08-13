//! 验证数学公式渲染：pulldown-cmark 的 ENABLE_MATH 对公式的解析
//! 及 setext heading 误判防护（protect_math_blocks）

use lnkwell_lib::markdown;

/// 用户原公式：`=` 独占一行，曾因 setext heading 误判导致 math 失效
#[test]
fn math_with_setext_equals_line() {
    let md = r#"
$$
d\mathbf X_t
=
\left[
\mathbf f(\mathbf X_t,t)
-g(t)^2\nabla_{\mathbf x}\log p_t(\mathbf X_t)
\right]dt
+g(t)\,d\overline{\mathbf W}_t,
\qquad dt<0.
$$
"#;
    let r = markdown::render_document(md, &|p| format!("md-file:///abs/{p}")).unwrap();
    let html = r.chunks.join("\n");
    println!("==== 用户原公式 ====");
    println!("{html}");
    println!("==== 结束 ====");
    assert!(html.contains("math-display"), "公式未被识别为 display math");
    // 公式内容应完整（含等号与向量偏导）
    assert!(html.contains(r"\nabla_{\mathbf x}"), "公式内容丢失");
    assert!(html.contains("d\\mathbf X_t ="), "等号合并后应紧跟前一行");
}

/// 对照：`=` 不独占一行时也应正常（回归防护）
#[test]
fn math_equals_inline_line() {
    let md = r#"
$$
d\mathbf X_t = \left[\mathbf f(\mathbf X_t,t) - g(t)^2\nabla_{\mathbf x}\log p_t(\mathbf X_t)\right]dt + g(t)\,d\overline{\mathbf W}_t, \qquad dt<0.
$$
"#;
    let r = markdown::render_document(md, &|p| format!("md-file:///abs/{p}")).unwrap();
    let html = r.chunks.join("\n");
    assert!(html.contains("math-display"), "单行公式未识别");
}

/// 常规多行公式（无 = 行干扰）
#[test]
fn math_normal_display() {
    let md = r#"
$$
\int_0^1 x^2 dx
$$
"#;
    let r = markdown::render_document(md, &|p| format!("md-file:///abs/{p}")).unwrap();
    let html = r.chunks.join("\n");
    assert!(html.contains("math-display"), "常规公式未识别");
}

/// 回归：math 块外的 `-` 列表项不受 protect_math_blocks 影响
#[test]
fn list_outside_math_untouched() {
    let md = "- 项目一\n- 项目二\n\n$$ a = b $$";
    let r = markdown::render_document(md, &|p| format!("md-file:///abs/{p}")).unwrap();
    let html = r.chunks.join("\n");
    println!("{html}");
    assert!(html.contains("<li>项目一"), "列表项被破坏");
    assert!(html.contains("<li>项目二"), "列表项被破坏");
    assert!(html.contains("math-display"), "单行 display math 未识别");
}

/// 回归：代码块内的 $$ 不应触发 math 保护（保持原样）
#[test]
fn code_block_math_untouched() {
    let md = "```\n$$\nx = y\n```";
    let r = markdown::render_document(md, &|p| format!("md-file:///abs/{p}")).unwrap();
    let html = r.chunks.join("\n");
    println!("{html}");
    assert!(!html.contains("math-display"), "代码块里的 $$ 不应被当公式");
}
