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
    /// 该标题所在的分块序号（前端跳转时一步渲染到该块，避免逐块探测）
    #[serde(default)]
    pub chunk: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderResult {
    /// 按 H2 切分、已消毒的 HTML 块
    pub chunks: Vec<String>,
    pub toc: Vec<TocItem>,
    pub title: Option<String>,
    /// 剥离 md 标记后的纯文本，供 FTS5 索引使用（方案 §7.3）
    pub plain: String,
    /// YAML front matter 元数据（title/date/tags，方案 §7.2），无则为 None
    #[serde(default)]
    pub frontmatter: Option<Frontmatter>,
}

/// YAML front matter 解析结果（方案 §7.2：渲染成小卡片而非正文）
#[derive(Debug, Clone, Serialize)]
pub struct Frontmatter {
    pub title: Option<String>,
    pub date: Option<String>,
    pub tags: Vec<String>,
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

/// 极简 YAML front matter 解析（方案 §7.2）。
/// 只处理 title/date/tags 三个字段，覆盖常见写法：
///   title: 标题
///   date: 2024-01-01
///   tags: [a, b, c]  /  tags: a, b, c  /  多行 - 列表
/// 不引入完整 YAML 解析器（依赖重、且 front matter 一般就是这几项）。
fn parse_frontmatter(yaml: &str) -> Frontmatter {
    let mut title = None;
    let mut date = None;
    let mut tags: Vec<String> = Vec::new();
    let mut in_tags = false;

    for raw in yaml.lines() {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // tags 的多行列表（缩进的 - 项）
        if in_tags {
            if let Some(item) = line.strip_prefix("- ") {
                let v = item.trim();
                if !v.is_empty() {
                    tags.push(unquote(v));
                }
                continue;
            } else if line.starts_with('-') {
                let v = line.trim_start_matches('-').trim();
                if !v.is_empty() {
                    tags.push(unquote(v));
                }
                continue;
            }
            // 不是 - 项了，说明 tags 多行列表结束
            in_tags = false;
        }

        // 找 key: value
        let (key, val) = match line.split_once(':') {
            Some((k, v)) => (k.trim().to_ascii_lowercase(), v.trim()),
            None => continue,
        };
        match key.as_str() {
            "title" => title = Some(unquote(val)),
            "date" | "created" => date = Some(unquote(val)),
            "tags" | "tag" | "categories" | "category" => {
                if val.is_empty() {
                    // 多行列表：后续行以 - 开头
                    in_tags = true;
                } else {
                    for t in parse_tag_value(val) {
                        if !t.is_empty() {
                            tags.push(t);
                        }
                    }
                }
            }
            _ => {}
        }
    }

    // 全空则视为无 front matter（避免渲染空卡片）
    if title.is_none() && date.is_none() && tags.is_empty() {
        return Frontmatter { title: None, date: None, tags: vec![] };
    }
    Frontmatter { title, date, tags }
}

/// 去掉首尾引号与空白
fn unquote(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 2 {
        let bytes = t.as_bytes();
        let f = bytes[0];
        let l = bytes[t.len() - 1];
        if (f == b'"' && l == b'"') || (f == b'\'' && l == b'\'') {
            return t[1..t.len() - 1].to_string();
        }
    }
    t.to_string()
}

/// 解析 tags 值：[a, b, c] / a, b, c / "a b" 等
fn parse_tag_value(val: &str) -> Vec<String> {
    let t = val.trim();
    if let Some(inner) = t.strip_prefix('[').and_then(|s| s.strip_suffix(']')) {
        return inner
            .split(',')
            .map(unquote)
            .filter(|s| !s.is_empty())
            .collect();
    }
    t.split(',')
        .map(unquote)
        .filter(|s| !s.is_empty())
        .collect()
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
        } else if (ch.is_whitespace() || ch == '-' || ch == '_')
            && !slug.ends_with('-')
            && !slug.is_empty()
        {
            slug.push('-');
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

/// 预处理：修复 display math 块内的 setext heading 误判，并支持 `[TOC]` 标记。
///
/// ### setext 误判
/// 用户习惯把 `=`（或 `-`）单独成行排版公式，如：
/// ```text
/// $$
/// d\mathbf X_t
/// =
/// \left[ ...
/// $$
/// ```
/// pulldown-cmark 会把 `=` 行识别为 setext H1 下划线，把 `$$` 块截断成
/// heading + 普通段落，math 完全失效。这里把 display math 块内孤立的
/// `=`/`-` 行合并到上一行末尾（LaTeX 语义不变），从根上消除误判。
///
/// ### `[TOC]` 标记（Typora 约定）
/// 独占一行的 `[TOC]` 不是标准 CommonMark 语法，默认渲染成普通段落。
/// 这里替换为占位 div，等 toc 收集完毕后生成真正的目录（见 render_document 尾部）。
const TOC_PLACEHOLDER: &str = "<div class=\"md-toc-insert\" data-md-toc></div>";

fn preprocess(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + 64);
    let mut in_math = false;
    for line in md.lines() {
        let t = line.trim();
        // 独占一行的 [TOC]：替换为占位（支持标题前后空白、大小写变体）
        if t.eq_ignore_ascii_case("[toc]") {
            out.push_str(TOC_PLACEHOLDER);
            out.push('\n');
            continue;
        }
        // 独占 `$$` 行：切换多行 display math 状态
        if t == "$$" {
            in_math = !in_math;
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // 单行 `$$...$$`：一次性 display，不进多行态
        if t.starts_with("$$") && t.ends_with("$$") && t.len() > 2 {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        // display math 块内孤立的 = 或 - 行：合并到上一行
        if in_math && (t == "=" || t == "-") {
            while out.ends_with('\n') {
                out.pop();
            }
            out.push(' ');
            out.push_str(t);
            out.push('\n');
            continue;
        }
        out.push_str(line);
        out.push('\n');
    }
    out
}

/// 渲染入口。`resolve_img` 把 md 里的相对图片路径解析为可注入 src 的字符串
/// （桌面端为 `md-file://` + 绝对路径；Android 为 content URL），外链原样保留。
pub fn render_document(
    md: &str,
    resolve_img: &dyn Fn(&str) -> String,
) -> AppResult<RenderResult> {
    // 先做预处理（setext 误判防护 + [TOC] 占位），再交给 pulldown-cmark
    let protected = preprocess(md);
    let events: Vec<Event> = Parser::new_ext(&protected, build_options()).collect();
    let mut toc: Vec<TocItem> = Vec::new();
    let mut used_slugs: std::collections::HashMap<String, usize> = Default::default();
    let mut title: Option<String> = None;
    let mut frontmatter: Option<Frontmatter> = None;

    // 第一遍：改写事件流 —— 标题注入 id、图片路径重写、front matter 解析
    let mut rewritten: Vec<Event> = Vec::with_capacity(events.len());
    let mut i = 0;
    while i < events.len() {
        match &events[i] {
            // front matter：解析为元数据，不渲染成正文（方案 §7.2）
            Event::Start(Tag::MetadataBlock(_)) => {
                let mut yaml = String::new();
                let mut j = i + 1;
                while j < events.len() {
                    match &events[j] {
                        Event::Text(t) => yaml.push_str(t),
                        Event::End(TagEnd::MetadataBlock(_)) => break,
                        _ => {}
                    }
                    j += 1;
                }
                if frontmatter.is_none() {
                    frontmatter = Some(parse_frontmatter(&yaml));
                }
                i = j + 1;
                continue;
            }
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
                toc.push(TocItem { level: n, text, id: id.clone(), chunk: 0 });
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

    // 第二遍：按 H2 分块渲染 HTML，同时给每个标题标注所属 chunk 序号。
    // 前端 TOC 跳转据此一步渲染到目标块（不再逐块探测，长文档秒跳）。
    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<Event> = Vec::new();
    let mut chunk_idx: usize = 0;
    let mut toc_iter = toc.iter_mut();
    let mut next_toc = toc_iter.next();
    for ev in rewritten {
        // H2 的起始标签已被我们改写为 Html 事件，借此识别分块边界
        if let Event::Html(ref raw) = ev {
            if raw.starts_with("<h2 ") && !cur.is_empty() {
                chunks.push(render_chunk(&cur));
                cur.clear();
                chunk_idx += 1;
            }
        }
        // 给当前标题标注 chunk：该标题的 Html 起始标签出现时，它就在当前块
        if let Event::Html(ref raw) = ev {
            if let Some(toc_item) = next_toc.as_mut() {
                if raw.starts_with(&format!("<h{} id=\"{}\"", toc_item.level, toc_item.id)) {
                    toc_item.chunk = chunk_idx;
                    next_toc = toc_iter.next();
                }
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

    // 用已收集的 toc 生成目录 HTML，替换 [TOC] 占位符（支持文档任意位置出现）。
    // 占位 div 经 ammonia 消毒后 data-md-toc 可能被序列化为 data-md-toc=""
    // （不保证与常量逐字节一致），故按 "md-toc-insert" 标记定位并替换整个 div。
    let toc_html = if toc.is_empty() {
        String::new()
    } else {
        build_toc_html(&toc)
    };
    const PLACEHOLDER_MARK: &str = "md-toc-insert";
    for chunk in chunks.iter_mut() {
        let mut start = 0;
        while let Some(rel) = chunk[start..].find(PLACEHOLDER_MARK) {
            let div_start = chunk[..start + rel].rfind("<div").unwrap_or(0);
            let div_end_rel = chunk[start + rel..].find("</div>");
            let div_end = match div_end_rel {
                Some(i) => start + rel + i + "</div>".len(),
                None => chunk.len(),
            };
            let mut new_chunk = String::with_capacity(chunk.len() + toc_html.len());
            new_chunk.push_str(&chunk[..div_start]);
            new_chunk.push_str(&toc_html);
            new_chunk.push_str(&chunk[div_end..]);
            *chunk = new_chunk;
            start = div_start + toc_html.len();
        }
    }

    let plain = extract_plain_text(md);
    Ok(RenderResult { chunks, toc, title, plain, frontmatter })
}

/// 生成文档内目录 HTML（Typora 风格 `[TOC]`）。
/// 用真正的嵌套 `<ul>/<li>` 树（栈式构建），浏览器对列表天然换行 + 缩进，
/// 不依赖任何自定义 CSS 的 display 行为，各层级视觉层次稳定。
/// 链接用 `md-toc://` 哨兵 scheme，前端拦截后走 tocJump 懒加载跳转
/// （与右侧目录抽屉同一机制，长文档大跨度也能跳）。
fn build_toc_html(toc: &[TocItem]) -> String {
    let mut out = String::from("<nav class=\"md-toc\" aria-label=\"目录\"><div class=\"md-toc-title\">目录</div>");
    if !toc.is_empty() {
        let mut i = 0;
        out.push_str(&emit_toc_items(toc, &mut i, 0));
    }
    out.push_str("</nav>");
    out
}

/// 递归输出 toc 的嵌套列表。`cur_level` 是当前列表归属的层级：
/// - `lvl > cur_level`：从更浅层进入 → 包 `<ul>` 递归处理
/// - `lvl == cur_level`：同级 → 输出 `<li>`，其后若紧跟更深项则递归子列表
/// - `lvl < cur_level`：回到更浅层 → 结束本层
///   toc 本身按文档顺序排列（先父后子），保证树构建正确。
fn emit_toc_items(toc: &[TocItem], i: &mut usize, cur_level: u8) -> String {
    let mut out = String::new();
    while *i < toc.len() {
        let lvl = toc[*i].level.clamp(1, 6);
        if lvl > cur_level {
            out.push_str("<ul class=\"md-toc-list\">");
            out.push_str(&emit_toc_items(toc, i, lvl));
            out.push_str("</ul>");
        } else if lvl == cur_level {
            let item = &toc[*i];
            *i += 1;
            out.push_str(&format!("<li class=\"toc-item toc-lvl-{lvl}\"><a href=\"md-toc://"));
            out.push_str(&item.id);
            out.push_str("\">");
            out.push_str(&escape_toc_text(&item.text));
            out.push_str("</a>");
            if *i < toc.len() && toc[*i].level.clamp(1, 6) > cur_level {
                out.push_str("<ul class=\"md-toc-list\">");
                out.push_str(&emit_toc_items(toc, i, toc[*i].level.clamp(1, 6)));
                out.push_str("</ul>");
            }
            out.push_str("</li>");
        } else {
            break;
        }
    }
    out
}

fn escape_toc_text(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
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
        .add_tags(&["details", "summary", "kbd", "mark", "input", "nav"])
        .add_tag_attributes("nav", &["class", "aria-label"])
        .add_tag_attributes("code", &["class"])
        .add_tag_attributes("span", &["class"])
        // data-md-toc 是 [TOC] 占位标记，必须保留才能在消毒后替换为真实目录
        .add_tag_attributes("div", &["class", "data-md-toc"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("a", &["id", "class", "href"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("img", &["width", "height", "alt", "title"])
        .url_schemes(
            ["http", "https", "mailto", "asset", "content", "md-file", "md-link", "md-toc", "data"]
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
    fn parses_frontmatter_title_date_tags() {
        let md = "---\ntitle: 扩散模型讲义\ndate: 2026-08-01\ntags: [AI, 生成模型, 数学]\n---\n\n# 正文标题\n\n内容";
        let r = render_document(md, &no_rewrite()).unwrap();
        let fm = r.frontmatter.expect("应有 front matter");
        assert_eq!(fm.title.as_deref(), Some("扩散模型讲义"));
        assert_eq!(fm.date.as_deref(), Some("2026-08-01"));
        assert_eq!(fm.tags, vec!["AI", "生成模型", "数学"]);
        // front matter 不应渲染成正文
        let joined = r.chunks.join("");
        assert!(!joined.contains("扩散模型讲义"), "front matter title 不应进正文");
    }

    #[test]
    fn parses_frontmatter_multiline_tags() {
        let md = "---\ntitle: T\ntags:\n  - a\n  - b\n  - c\n---\n\n# H\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        let fm = r.frontmatter.expect("应有 front matter");
        assert_eq!(fm.tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parses_frontmatter_comma_tags_and_quotes() {
        let md = "---\ntitle: \"带引号标题\"\ndate: '2026-08-02'\ntags: x, y, z\n---\n\n# H\n";
        let r = render_document(md, &no_rewrite()).unwrap();
        let fm = r.frontmatter.expect("应有 front matter");
        assert_eq!(fm.title.as_deref(), Some("带引号标题"));
        assert_eq!(fm.date.as_deref(), Some("2026-08-02"));
        assert_eq!(fm.tags, vec!["x", "y", "z"]);
    }

    #[test]
    fn no_frontmatter_when_absent() {
        let md = "# 只有标题\n\n正文";
        let r = render_document(md, &no_rewrite()).unwrap();
        assert!(r.frontmatter.is_none(), "无 front matter 应为 None");
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
