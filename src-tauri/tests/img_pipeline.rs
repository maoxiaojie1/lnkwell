//! 验证图片管线：read_doc 渲染出 md-file:// 哨兵 src，
//! Rust 端按该 src 去掉前缀后能读出图片字节并编码为 data URL。

use base64::Engine as _;
use lnkwell_lib::markdown;

#[test]
fn md_file_src_resolves_and_reads() {
    // 模拟桌面 read_doc 的解析器
    let doc = r"D:\code\lnkwell\fixtures\desktop-test\test-img.md";
    let resolver = |rel: &str| {
        let base = std::path::Path::new(doc).parent().unwrap();
        let joined = base.join(rel);
        // 去 . 段
        let norm: std::path::PathBuf = joined.components().collect();
        format!("{}{}", markdown::LOCAL_FILE_SCHEME, norm.to_string_lossy().replace('\\', "/"))
    };

    let md = "![测试图](./img/pic1.png)";
    let r = markdown::render_document(md, &resolver).unwrap();
    let joined = r.chunks.join("");
    println!("rendered html: {joined}");

    // 提取 src
    let src = joined
        .split("src=\"")
        .nth(1)
        .and_then(|s| s.split('"').next())
        .expect("img src missing");
    assert!(src.starts_with("md-file://"), "期望 md-file:// 前缀，实际: {src}");

    // 前端 slice 掉前缀得到绝对路径
    let abs = &src["md-file://".len()..];
    assert!(std::path::Path::new(abs).is_absolute(), "必须是绝对路径: {abs}");

    // 模拟 read_image 命令内部：读字节 + base64
    let bytes = std::fs::read(abs).expect("图片必须能读");
    assert_eq!(&bytes[..4], &[0x89, b'P', b'N', b'G']);

    let b64 = base64::engine::general_purpose::STANDARD.encode(&bytes);
    let data_url = format!("data:image/png;base64,{b64}");
    println!("data URL 前缀: {}", &data_url[..40]);
    assert!(data_url.starts_with("data:image/png;base64,"));

    // 前端能解码回原始字节（模拟 img.src = dataUrl 后浏览器解码）
    let payload = &data_url["data:image/png;base64,".len()..];
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(payload)
        .unwrap();
    assert_eq!(decoded, bytes);
}
