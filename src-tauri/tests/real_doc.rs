//! 真实路径下的图片管线（改进计划 A1 之后重写）
//!
//! 新契约：渲染期只产出 `md-rel://<原始相对路径>` 哨兵，平台解析交给
//! `resolve_entry`（桌面实现 `resolve_entry_inner`）。因此这里验证三件事：
//! 1. 非 ASCII 路径经 `escape_href` percent-encode 后可逆；
//! 2. 哨兵可以解析成真实存在的文件；
//! 3. 上级目录引用被规范化，不残留 `..`。

mod common;

use lnkwell_lib::markdown::render_document;
use lnkwell_lib::vault::desktop::resolve_entry_inner;
use std::path::Path;

const SENTINEL: &str = "md-rel://";

fn decode_rel(src: &str) -> String {
    assert!(src.starts_with(SENTINEL), "期望 md-rel:// 哨兵，实际: {src}");
    percent_encoding::percent_decode_str(&src[SENTINEL.len()..])
        .decode_utf8()
        .expect("哨兵内容必须能解回合法 UTF-8")
        .into_owned()
}

#[test]
fn chinese_relative_path_is_reversible() {
    // 含空格的路径必须用尖括号包裹，否则按 CommonMark 规则 dest 会在空格处截断
    let r = render_document("![a](<./图片/中文 图.png>)");
    let src = common::first_img_src(&r.chunks.concat());
    assert_eq!(decode_rel(&src), "./图片/中文 图.png");
}

#[test]
fn relative_image_resolves_to_real_png() {
    let doc = common::repo_path("fixtures/desktop-test/test-img.md");
    let doc_str = doc.to_string_lossy().into_owned();

    let r = render_document("![测试图](./img/pic1.png)");
    let rel = decode_rel(&common::first_img_src(&r.chunks.concat()));

    let abs = resolve_entry_inner(&doc_str, &rel);
    assert!(Path::new(&abs).is_file(), "解析结果应真实存在: {abs}");
    let bytes = std::fs::read(&abs).expect("图片必须能读");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);
}

#[test]
fn parent_dir_reference_is_normalized() {
    let doc = if cfg!(windows) {
        r"D:\docs\note\readme.md"
    } else {
        "/docs/note/readme.md"
    };
    let abs = resolve_entry_inner(doc, "../assets/x.png");
    assert!(!abs.contains(".."), "不应残留上级引用: {abs}");
    assert!(
        abs.ends_with("assets/x.png") || abs.ends_with("assets\\x.png"),
        "应落在 assets 下: {abs}"
    );
}

#[test]
fn external_and_data_urls_are_untouched() {
    let md = "![a](https://x.com/a.png)\n\n![b](data:image/png;base64,AA)\n";
    let html = render_document(md).chunks.concat();
    assert!(html.contains("https://x.com/a.png"));
    assert!(html.contains("data:image/png;base64,AA"));
    assert!(!html.contains(SENTINEL), "外链与 data URL 不应被加哨兵");
}
