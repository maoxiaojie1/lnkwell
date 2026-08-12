//! Markdown 渲染管线（方案 §5）：pulldown-cmark 解析 → 标题注入 anchor id →
//! 按 H2 分块 → 图片路径重写 → ammonia 消毒（§9.4）。

use crate::error::{AppError, AppResult};
use encoding_rs::{GB18030, UTF_8};
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

/// 单文件大小上限（方案 §5.6）：超过即拒绝，避免 OOM。
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// 桌面端图片哨兵 scheme：Rust 把相对路径解析成绝对路径后加此前缀，
/// 前端在增强管线里用 convertFileSrc 换成 WebView 可加载的 asset URL。
/// 这样做的原因：`D:\img\a.png` 中的 `d:` 会被 ammonia 当作未知 URL scheme 剥掉。
pub const LOCAL_FILE_SCHEME: &str = "md-file://";

#[derive(Debug, Clone, Serialize)]
pub struct TocItem {
    pub level: u8,
    pub text: String,
    pub id: String,
}

#[derive(Debug, Serialize)]
pub struct RenderResult {
    /// 按 H2 切分、已消毒的 HTML 块
    pub chunks: Vec<String>,
    pub toc: Vec<TocItem>,
    pub title: Option<String>,
    /// 剥离 md 标记后的纯文本，供 FTS5 索引使用（方案 §7.3）
    pub plain: String,
}

/// GFM 选项（方案 §5.2）。刻意不开 ENABLE_SMART_PUNCTUATION，
/// 避免 `--no-default-features` 被转成 en dash 破坏可复制性。
pub fn build_options() -> Options {
    let mut opts = Options::empty();
    opts.insert(Options::ENABLE_TABLES);
    opts.insert(Options::ENABLE_FOOTNOTES);
    opts.insert(Options::ENABLE_STRIKETHROUGH);
    opts.insert(Options::ENABLE_TASKLISTS);
    opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);
    opts.insert(Options::ENABLE_YAML_STYLE_METADATA_BLOCKS);
    // 数学公式：把 $...$ / $$...$$ 解析为 <span class="math"> 供 KaTeX 识别（§5.2）
    opts.insert(Options::ENABLE_MATH);
    opts
}

/// 文本解码（方案 §5.6）：UTF-8(BOM) → UTF-8 → GB18030 → UTF-8 有损。
pub fn decode_text(bytes: &[u8]) -> AppResult<(String, &'static str)> {
    if let Some(stripped) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        if let Ok(s) = std::str::from_utf8(stripped) {
            return Ok((s.to_owned(), "UTF-8 (BOM)"));
        }
    }
    if let Ok(s) = std::str::from_utf8(bytes) {
        return Ok((s.to_owned(), "UTF-8"));
    }
    let (cow, _, had_errors) = GB18030.decode(bytes);
    if !had_errors {
        return Ok((cow.into_owned(), "GB18030"));
    }
    Ok((UTF_8.decode(bytes).0.into_owned(), "UTF-8 (lossy)"))
}

/// CRLF / 孤 CR 统一为 LF（代码块内的 \r 会原样进 <pre>，见方案 §5.6）。
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// 抽取纯文本供全文索引（方案 §7.3）。
pub fn extract_plain_text(md: &str) -> String {
    let mut out = String::with_capacity(md.len());
    for ev in Parser::new_ext(md, build_options()) {
        match ev {
            Event::Text(t) | Event::Code(t) => out.push_str(&t),
            Event::SoftBreak | Event::HardBreak => out.push(' '),
            Event::End(_) => out.push(' '),
            _ => {}
        }
    }
    out
}

fn heading_level_num(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// 生成 URL-safe slug：保留字母数字与 CJK，空白转连字符。
fn slugify(text: &str, used: &mut std::collections::HashMap<String, usize>) -> String {
    let mut slug = String::with_capacity(text.len());
    for ch in text.chars() {
        if ch.is_alphanumeric() {
            slug.extend(ch.to_lowercase());
        } else if ch.is_whitespace() || ch == '-' || ch == '_' {
            if !slug.ends_with('-') && !slug.is_empty() {
                slug.push('-');
            }
        }
    }
    let slug = slug.trim_matches('-').to_owned();
    let base = if slug.is_empty() { "section".to_owned() } else { slug };
    let count = used.entry(base.clone()).or_insert(0);
    *count += 1;
    if *count == 1 {
        base
    } else {
        format!("{base}-{}", *count - 1)
    }
}

/// 渲染入口。`resolve_img` 把 md 里的相对图片路径解析为可注入 src 的字符串
/// （桌面端为 `md-file://` + 绝对路径；Android 为 content URL），外链原样保留。
pub fn render_document(
    md: &str,
    resolve_img: &dyn Fn(&str) -> String,
) -> AppResult<RenderResult> {
    let events: Vec<Event> = Parser::new_ext(md, build_options()).collect();
    let mut toc: Vec<TocItem> = Vec::new();
    let mut used_slugs: std::collections::HashMap<String, usize> = Default::default();
    let mut title: Option<String> = None;

    // 第一遍：改写事件流 —— 标题注入 id、图片路径重写
    let mut rewritten: Vec<Event> = Vec::with_capacity(events.len());
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            Event::Start(Tag::Heading { level, .. }) => {
                let lvl = *level;
                // 收集标题内文本用于 slug 与 TOC
                let mut text = String::new();
                let mut j = i + 1;
                while j < events.len() {
                    match &events[j] {
                        Event::End(TagEnd::Heading(_)) => break,
                        Event::Text(t) | Event::Code(t) => text.push_str(t),
                        _ => {}
                    }
                    j += 1;
                }
                let id = slugify(&text, &mut used_slugs);
                let n = heading_level_num(lvl);
                if n == 1 && title.is_none() {
                    title = Some(text.clone());
                }
                toc.push(TocItem { level: n, text, id: id.clone() });
                rewritten.push(Event::Html(CowStr::from(format!("<h{n} id=\"{id}\">"))));
                // 拷贝标题内部事件
                for ev in &events[i + 1..j] {
                    rewritten.push(rewrite_event(ev.clone(), resolve_img));
                }
                rewritten.push(Event::Html(CowStr::from(format!("</h{n}>"))));
                i = j + 1;
            }
            other => {
                rewritten.push(rewrite_event(other.clone(), resolve_img));
                i += 1;
            }
        }
    }

    // 第二遍：按 H2 分块渲染 HTML
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<Event> = Vec::new();
    for ev in rewritten {
        // H2 的起始标签已被我们改写为 Html 事件，借此识别分块边界
        if let Event::Html(ref raw) = ev {
            if raw.starts_with("<h2 ") && !cur.is_empty() {
                chunks.push(render_chunk(&cur));
                cur.clear();
            }
        }
        cur.push(ev);
    }
    if !cur.is_empty() {
        chunks.push(render_chunk(&cur));
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }

    let plain = extract_plain_text(md);
    Ok(RenderResult { chunks, toc, title, plain })
}

fn rewrite_event(ev: Event<'_>, resolve_img: &dyn Fn(&str) -> String) -> Event<'static> {
    match ev {
        Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
            let resolved = if dest_url.starts_with("http://")
                || dest_url.starts_with("https://")
                || dest_url.starts_with("data:")
            {
                dest_url.into_string()
            } else {
                resolve_img(&dest_url)
            };
            Event::Start(Tag::Image {
                link_type,
                dest_url: CowStr::from(resolved),
                title,
                id,
            })
            .into_static()
        }
        Event::Start(Tag::Link { link_type, dest_url, title, id }) => {
            // md 互跳链接：相对 .md 路径加哨兵 scheme，前端拦截转内部导航（方案 §7.1）
            let lower = dest_url.to_ascii_lowercase();
            let is_md_link = !dest_url.starts_with("http://")
                && !dest_url.starts_with("https://")
                && !dest_url.starts_with('#')
                && !dest_url.starts_with("mailto:")
                && (lower.ends_with(".md")
                    || lower.ends_with(".markdown")
                    || lower.ends_with(".mdx")
                    || lower.contains(".md#")
                    || lower.contains(".markdown#"));
            let url: CowStr<'static> = if is_md_link {
                CowStr::from(format!("md-link://{dest_url}"))
            } else {
                // 函数返回 Event<'static>，原样透传的 CowStr 可能借用输入，必须 owned
                CowStr::from(dest_url.into_string())
            };
            Event::Start(Tag::Link { link_type, dest_url: url, title, id }).into_static()
        }
        other => other.into_static(),
    }
}

fn render_chunk(events: &[Event]) -> String {
    let mut raw = String::with_capacity(events.len() * 64);
    html::push_html(&mut raw, events.iter().cloned());
    sanitize(&raw)
}

/// HTML 消毒（方案 §9.4）：ammonia 默认剥掉 script/iframe/on* 属性/javascript: URL。
/// 必须显式加回 h1-h6 的 id（TOC 跳转与阅读进度依赖），以及 hljs/mermaid 需要的 class。
pub fn sanitize(html_str: &str) -> String {
    ammonia::Builder::default()
        // input 不在 ammonia 默认允许集中，但任务列表复选框依赖它（§5.2 ENABLE_TASKLISTS）
        .add_tags(&["details", "summary", "kbd", "mark", "input"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("span", &["class"])
        .add_tag_attributes("div", &["class"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("a", &["id"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("img", &["width", "height", "alt", "title"])
        .url_schemes(
            ["http", "https", "mailto", "asset", "content", "md-file", "md-link", "data"]
                .iter()
                .cloned()
                .collect(),
        )
        .clean(html_str)
        .to_string()
}

/// 供命令层使用的便捷错误转换
pub fn too_large(size: u64) -> AppError {
    AppError::TooLarge(size)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn no_rewrite() -> impl Fn(&str) -> String {
        |s: &str| s.to_owned()
    }

    #[test]
    fn decodes_utf8_bom() {
        let bytes = b"\xEF\xBB\xBF# Title\n".to_vec();
        let (text, enc) = decode_text(&bytes).unwrap();
        assert_eq!(enc, "UTF-8 (BOM)");
        assert!(text.starts_with("# Title"));
    }

    #[test]
    fn decodes_gbk() {
        // "中文" 的 GBK 编码
        let bytes = vec![0xD6, 0xD0, 0xCE, 0xC4];
        let (text, enc) = decode_text(&bytes).unwrap();
        assert_eq!(enc, "GB18030");
        assert_eq!(text, "中文");
    }

    #[test]
    fn normalizes_crlf() {
        assert_eq!(normalize_newlines("a\r\nb\rc"), "a\nb\nc");
    }

    #[test]
    fn heading_gets_anchor_id_and_toc() {
        let r = render_document("# Hello World\n\n## 第二节\n\ntext\n", &no_rewrite()).unwrap();
        assert!(r.chunks.iter().any(|c| c.contains("id=\"hello-world\"")));
        assert_eq!(r.title.as_deref(), Some("Hello World"));
        assert!(r.toc.iter().any(|t| t.text == "第二节" && t.level == 2));
    }

    #[test]
    fn chunks_split_on_h2() {
        let md = "# A\n\nintro\n\n## One\n\n1\n\n## Two\n\n2\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        assert_eq!(r.chunks.len(), 3);
    }

    #[test]
    fn strips_xss() {
        let md = "safe\n\n<script>alert(1)</script>\n\n<img src=x onerror=\"alert(1)\">\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        let joined = r.chunks.join("");
        assert!(!joined.contains("<script"));
        assert!(!joined.contains("onerror"));
    }

    #[test]
    fn keeps_tasklist_checkbox() {
        let md = "- [x] done\n- [ ] todo\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        let joined = r.chunks.join("");
        assert!(joined.contains("type=\"checkbox\""));
    }

    #[test]
    fn rewrites_relative_images() {
        let md = "![a](./img/a.png)\n\n![b](https://x.com/b.png)\n";
        let r = render_document(md, &|p| format!("{LOCAL_FILE_SCHEME}/abs/{p}")).unwrap();
        let joined = r.chunks.join("");
        assert!(joined.contains("md-file:///abs/./img/a.png"));
        assert!(joined.contains("https://x.com/b.png"));
    }

    #[test]
    fn marks_internal_md_links() {
        let md = "[见架构](./arch.md) and [web](https://x.com)\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        let joined = r.chunks.join("");
        assert!(joined.contains("md-link://./arch.md"));
        assert!(joined.contains("https://x.com"));
    }

    #[test]
    fn plain_text_strips_markup() {
        let md = "# 标题\n\n**加粗** 和 `code`\n";
        let plain = extract_plain_text(md);
        assert!(plain.contains("标题"));
        assert!(plain.contains("加粗"));
        assert!(!plain.contains("**"));
        assert!(!plain.contains('#'));
    }

    #[test]
    fn math_inline_renders_math_span() {
        let md = "行内公式 $E = mc^2$ 结束";
        let out = render_document(md, &|p| p.to_owned()).expect("render ok");
        let html: String = out.chunks.concat();
        assert!(
            html.contains("math-inline") || html.contains("class=\"math\""),
            "期望 math span，实际: {html}"
        );
    }

    #[test]
    fn math_block_renders_display_span() {
        let md = "$$\n\\int_0^1 x dx\n$$";
        let out = render_document(md, &|p| p.to_owned()).expect("render ok");
        let html: String = out.chunks.concat();
        assert!(
            html.contains("math-display") || html.contains("class=\"math\""),
            "期望 math display span，实际: {html}"
        );
    }
}
