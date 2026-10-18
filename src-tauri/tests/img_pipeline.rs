//! 图片管线端到端：渲染出 `md-rel://` 哨兵 → 解析成绝对路径 → 读字节 →
//! base64 data URL → 解码回原始字节（改进计划 A1）。

mod common;

use base64::Engine as _;
use lnkwell_lib::markdown::render_document;
use lnkwell_lib::vault::desktop::resolve_entry_inner;

#[test]
fn rendered_src_is_the_raw_relative_path() {
    let html = render_document("![测试图](./img/pic1.png)").chunks.concat();
    let src = common::first_img_src(&html);
    // 渲染期只打哨兵，不解析平台路径
    assert_eq!(src, "md-rel://./img/pic1.png");
}

#[test]
fn relative_src_resolves_and_roundtrips_data_url() {
    let doc = common::repo_path("fixtures/desktop-test/test-img.md");
    let doc_str = doc.to_string_lossy().into_owned();

    // 与 test-img.md 同目录的真实图片
    let abs = resolve_entry_inner(&doc_str, "./img/pic1.png");
    let bytes = std::fs::read(&abs).expect("图片必须能读");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);

    // 模拟 Rust read_image 的编码过程
    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:image/png;base64,{b64}");
    assert!(data_url.starts_with("data:image/png;base64,"));

    // 浏览器解码路径：data URL payload 必须能还原字节
    let payload = &data_url["data:image/png;base64,".len()..];
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .expect("base64 必须可解");
    assert_eq!(decoded, bytes);
}
