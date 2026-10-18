//! 集成测试共享助手（改进计划 C3）。
//!
//! 这里集中处理「fixture 定位」与「合成文档」，避免各测试文件继续依赖
//! 开发者机器上的绝对路径（例如 `D:\code\扩散模型\...`）——那样这些测试在 CI 上必然失败。
#![allow(dead_code)]

use lnkwell_lib::markdown::{decode_text, normalize_newlines, render_document, RenderResult};
use std::path::{Path, PathBuf};

/// 仓库内相对路径 → 绝对路径（以 `src-tauri` 为基准）
pub fn repo_path(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("..").join(rel)
}

/// `fixtures/cases/<name>` 的绝对路径
pub fn case_path(name: &str) -> PathBuf {
    repo_path(&format!("fixtures/cases/{name}"))
}

/// 读取 fixture 并走「解码 → 换行归一化 → 渲染」的完整管线
pub fn render_case(name: &str) -> RenderResult {
    let bytes =
        std::fs::read(case_path(name)).unwrap_or_else(|e| panic!("读取 fixture {name} 失败: {e}"));
    let (text, _enc) = decode_text(&bytes).unwrap_or_else(|e| panic!("解码 {name} 失败: {e}"));
    render_document(&normalize_newlines(&text))
}

/// 合成一篇带 `[TOC]` 与 `n` 个 H2 的文档。
/// 用于替代「机器本地大文档」，让 TOC / 分块类测试可在任意环境运行。
pub fn synthetic_toc_doc(n: usize) -> String {
    let mut md = String::from("# 合成讲义\n\n[TOC]\n\n## 导言\n\n导言正文。\n\n");
    for i in 0..n {
        md.push_str(&format!("## 第 {i} 节\n\n小节正文 {i}。\n\n"));
    }
    md
}

/// 取出 HTML 里第一个 `<img>` 的地址。
/// 相对引用放在 `data-src`（浏览器不会预取），外链放在 `src`。
pub fn first_img_src(html: &str) -> String {
    let after = html
        .split("<img")
        .nth(1)
        .unwrap_or_else(|| panic!("HTML 无 img: {html}"));
    let pick = |attr: &str| {
        after
            .split(&format!("{attr}=\""))
            .nth(1)
            .and_then(|s| s.split('"').next())
            .map(|s| s.to_string())
    };
    pick("data-src")
        .or_else(|| pick("src"))
        .unwrap_or_else(|| panic!("img 缺地址属性: {html}"))
}
