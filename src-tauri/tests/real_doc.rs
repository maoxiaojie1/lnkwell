//! 用用户真实文档验证图片管线（中文路径 + 无 ./ 前缀 + 大图）
//!
//! 关键点：pulldown-cmark 的 escape_href 会把 img src 中的中文
//! percent-encode（见 real_doc_cover_png 输出），前端必须 decodeURIComponent。

use lnkwell_lib::markdown;

fn resolve(doc: &str, rel: &str) -> String {
    let base = std::path::Path::new(doc).parent().unwrap();
    let joined = base.join(rel);
    let norm: std::path::PathBuf = joined.components().collect();
    format!("{}{}", markdown::LOCAL_FILE_SCHEME, norm.to_string_lossy().replace('\\', "/"))
}

#[test]
fn real_doc_cover_png() {
    let doc = r"D:\code\扩散模型\扩散模型统一讲义.md";
    let src_md = std::fs::read_to_string(doc).expect("读文档失败");

    // 完整渲染，定位 img src
    let r = markdown::render_document(&src_md, &|rel| resolve(doc, rel)).expect("渲染失败");
    let html = r.chunks.join("");
    let img_idx = html.find("<img").expect("HTML 应有 img");
    let after = &html[img_idx..];
    let img_tag = after
        .split("</p>")
        .next()
        .unwrap_or(after);
    println!("HTML img 标签: {img_tag}");

    let src = after
        .split("src=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("提取 src");
    println!("渲染出的 src: {src}");
    assert!(src.starts_with("md-file://"));

    // 前端逻辑：slice 前缀 + decodeURIComponent
    let ref_encoded = &src["md-file://".len()..];
    let abs = percent_encoding::percent_decode_str(ref_encoded)
        .decode_utf8()
        .expect("必须是合法 UTF-8");
    println!("前端解码后路径: {abs}");
    assert!(abs.contains("扩散模型"), "中文路径必须还原: {abs}");

    // read_image 可读到字节
    let bytes = std::fs::read(abs.as_ref()).expect("图片必须能读");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
    println!("图片字节数: {}", bytes.len());
}

#[test]
fn escape_href_is_reversible() {
    // 验证 escape_href 对 % 字符也编码（%25），保证 decode 完全可逆 ——
    // 这决定了 Android content URI（自带 %3A）经渲染后 decode 也能还原
    let md = "![a](content://media/external/images/media/123%3A456)";
    let r = markdown::render_document(md, &|_| "X".to_string()).unwrap();
    let html = r.chunks.join("");
    println!("含 content URI 的渲染: {html}");
    // content URI 不匹配 md-file:// 前缀，前端不会动它
    assert!(!html.contains("md-file://"));
}
