//! Markdown 渲染管线（方案 §5）：pulldown-cmark 解析 → 标题注入 anchor id →
//! 按顶层 H2 分块 → 图片/链接改写 → ammonia 消毒（§9.4）。
//!
//! 设计约束（改进计划 A1/A8/A9/B1）：
//! - 图片相对路径**不在渲染期解析**：统一改写成 `md-rel://<原始相对路径>` 哨兵，
//!   由 `resolve_entry` 命令在需要时按平台解析（SAF 的 URI 解析是 async，渲染期做不到）。
//! - 分块只在**容器深度为 0** 的 H2 处发生，避免把 blockquote / details 切成两半。
//! - 预处理必须感知围栏代码块，否则代码块里的 `[TOC]` / `$$` 会被误改。
//! - 阅读路径不产出纯文本（`extract_index` 负责索引），避免重复解析与全文 IPC。

use crate::error::{AppError, AppResult};
use encoding_rs::{GB18030, UTF_8};
use pulldown_cmark::{html, CowStr, Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

/// 单文件大小上限（方案 §5.6）：超过即拒绝，避免 OOM。
pub const MAX_FILE_SIZE: u64 = 10 * 1024 * 1024;

/// 文档内相对引用的哨兵 scheme（改进计划 A1）。
/// Rust 不再把相对路径解析成绝对路径/content URI，只标记「这是相对引用」，
/// 前端拿到原文后调用 `read_image_in_doc` / `resolve_entry` 按平台解析。
/// 这样做的原因：`D:\img\a.png` 里的 `d:` 会被 ammonia 当成未知 scheme 剥掉，
/// 且 Android 的 SAF 解析无法在同步渲染回调里完成。
pub const LOCAL_REL_SCHEME: &str = "md-rel://";

/// 文档互跳链接的哨兵 scheme（方案 §7.1）
pub const INTERNAL_LINK_SCHEME: &str = "md-link://";

/// 文档内目录跳转的哨兵 scheme（Typora `[TOC]`）
pub const TOC_LINK_SCHEME: &str = "md-toc://";

/// `[TOC]` 占位符。只用 class（不用 data-* 属性）：
/// ammonia 序列化空值属性时会变成 `data-x=""`，导致字符串定位失效。
const TOC_PLACEHOLDER: &str = "<div class=\"lnkw-toc-insert\"></div>";

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
    /// 按顶层 H2 切分、已消毒的 HTML 块
    pub chunks: Vec<String>,
    pub toc: Vec<TocItem>,
    pub title: Option<String>,
    /// YAML front matter 元数据（title/date/tags，方案 §7.2），无则为 None
    #[serde(default)]
    pub frontmatter: Option<Frontmatter>,
}

/// 索引抽取结果（改进计划 A5/B1）。只做一次 parser 遍历，
/// 不生成 HTML、不分块、不做 ammonia 消毒。
#[derive(Debug, Clone, Serialize)]
pub struct IndexDoc {
    pub title: Option<String>,
    pub plain: String,
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

/// 抽取纯文本供全文索引（方案 §7.3）。保留为独立函数便于测试与复用。
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

/// 索引抽取：一次遍历同时拿标题与纯文本（改进计划 A5）。
/// 标题优先级：front matter 的 title > 首个 H1。
pub fn extract_index(md: &str) -> IndexDoc {
    let protected = preprocess(md);
    let mut plain = String::with_capacity(protected.len());
    let mut fm_title: Option<String> = None;
    let mut heading: Option<(HeadingLevel, String)> = None;
    let mut first_h1: Option<String> = None;
    let mut in_meta = false;
    let mut yaml = String::new();

    for ev in Parser::new_ext(&protected, build_options()) {
        match ev {
            Event::Start(Tag::MetadataBlock(_)) => {
                in_meta = true;
                yaml.clear();
            }
            Event::End(TagEnd::MetadataBlock(_)) => {
                in_meta = false;
                if fm_title.is_none() {
                    fm_title = parse_frontmatter(&yaml).title;
                }
            }
            Event::Start(Tag::Heading { level, .. }) => {
                heading = Some((level, String::new()));
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some((level, text)) = heading.take() {
                    if level == HeadingLevel::H1 && first_h1.is_none() {
                        first_h1 = Some(text);
                    }
                }
            }
            Event::Text(t) | Event::Code(t) => {
                if in_meta {
                    yaml.push_str(&t);
                } else {
                    if let Some((_, text)) = heading.as_mut() {
                        text.push_str(&t);
                    }
                    plain.push_str(&t);
                }
            }
            Event::SoftBreak | Event::HardBreak => plain.push(' '),
            Event::End(_) => plain.push(' '),
            _ => {}
        }
    }

    IndexDoc { title: fm_title.or(first_h1), plain }
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

/// 围栏代码块的开标记：返回 (字符, 长度)
fn fence_open(line: &str) -> Option<(char, usize)> {
    let t = line.trim_end();
    for ch in ['`', '~'] {
        if t.starts_with(ch) {
            let n = t.chars().take_while(|c| *c == ch).count();
            if n >= 3 {
                return Some((ch, n));
            }
        }
    }
    None
}

/// 围栏是否闭合：同字符重复、长度不短于开标记、其后仅空白
fn fence_close(line: &str, ch: char, open_len: usize) -> bool {
    let t = line.trim();
    let n = t.chars().take_while(|c| *c == ch).count();
    n >= open_len && t.chars().all(|c| c == ch || c.is_whitespace())
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
///
/// ### 围栏代码块（改进计划 A9）
/// 围栏内的内容必须原样保留：否则示例代码里的 `[TOC]` 会被替换成占位符，
/// 一个孤立的 `$$` 还会翻转 `in_math` 状态、波及之后整篇文档。
fn preprocess(md: &str) -> String {
    let mut out = String::with_capacity(md.len() + 64);
    let mut in_math = false;
    // 当前所处围栏：(定界字符, 开标记长度)
    let mut fence: Option<(char, usize)> = None;

    for line in md.lines() {
        let t = line.trim();

        // 围栏内：原样输出，只检测闭合
        if let Some((ch, open_len)) = fence {
            out.push_str(line);
            out.push('\n');
            if fence_close(t, ch, open_len) {
                fence = None;
            }
            continue;
        }
        // 围栏开启：原样输出，进入围栏态
        if let Some((ch, open_len)) = fence_open(t) {
            fence = Some((ch, open_len));
            out.push_str(line);
            out.push('\n');
            continue;
        }

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

/// 渲染入口。
///
/// 与旧版签名的差异（改进计划 A1）：不再接收 `resolve_img` 回调。
/// 图片相对路径统一改写成 `md-rel://` 哨兵，由上层按平台解析。
pub fn render_document(md: &str) -> RenderResult {
    let protected = preprocess(md);
    let events: Vec<Event> = Parser::new_ext(&protected, build_options()).collect();

    let mut chunks: Vec<String> = Vec::new();
    let mut cur: Vec<Event<'static>> = Vec::new();
    let mut chunk_idx: usize = 0;
    let mut toc: Vec<TocItem> = Vec::new();
    let mut used_slugs: std::collections::HashMap<String, usize> = Default::default();
    let mut title: Option<String> = None;
    let mut frontmatter: Option<Frontmatter> = None;
    // 容器深度：只有深度 0 的 H2 才允许切块（改进计划 A8）
    let mut depth: usize = 0;

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
                let n = heading_level_num(lvl);
                // 顶层 H2 且当前块非空 → 在此切块
                if n == 2 && depth == 0 && !cur.is_empty() {
                    chunks.push(render_chunk(&cur));
                    cur.clear();
                    chunk_idx += 1;
                }
                let id = slugify(&text, &mut used_slugs);
                if n == 1 && title.is_none() {
                    title = Some(text.clone());
                }
                toc.push(TocItem { level: n, text, id: id.clone(), chunk: chunk_idx });
                cur.push(Event::Html(CowStr::from(format!("<h{n} id=\"{id}\">"))));
                // 拷贝标题内部事件
                for ev in &events[i + 1..j] {
                    cur.push(rewrite_event(ev.clone()));
                }
                cur.push(Event::Html(CowStr::from(format!("</h{n}>"))));
                i = j + 1;
            }
            other => {
                match other {
                    Event::Start(_) => depth += 1,
                    Event::End(_) => depth = depth.saturating_sub(1),
                    _ => {}
                }
                cur.push(rewrite_event(other.clone()));
                i += 1;
            }
        }
    }
    if !cur.is_empty() {
        chunks.push(render_chunk(&cur));
    }
    if chunks.is_empty() {
        chunks.push(String::new());
    }

    // 用已收集的 toc 生成目录 HTML，替换 [TOC] 占位符（支持文档任意位置出现）。
    // 占位符只由 class 标记，ammonia 序列化后仍与常量逐字节一致。
    let toc_html = if toc.is_empty() { String::new() } else { build_toc_html(&toc) };
    for chunk in chunks.iter_mut() {
        if chunk.contains(TOC_PLACEHOLDER) {
            *chunk = chunk.replace(TOC_PLACEHOLDER, &toc_html);
        }
    }

    RenderResult { chunks, toc, title, frontmatter }
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
            out.push_str(&format!("<li class=\"toc-item toc-lvl-{lvl}\"><a href=\"{TOC_LINK_SCHEME}"));
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

/// 事件改写（改进计划 A1）：图片相对路径 → `md-rel://` 哨兵，md 互跳 → `md-link://` 哨兵。
/// 外链与 data URL 原样保留；绝对路径同样交给上层按平台解析（桌面端会直接命中）。
fn rewrite_event(ev: Event<'_>) -> Event<'static> {
    match ev {
        Event::Start(Tag::Image { link_type, dest_url, title, id }) => {
            let resolved = if dest_url.starts_with("http://")
                || dest_url.starts_with("https://")
                || dest_url.starts_with("data:")
            {
                dest_url.into_string()
            } else {
                format!("{LOCAL_REL_SCHEME}{dest_url}")
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
                CowStr::from(format!("{INTERNAL_LINK_SCHEME}{dest_url}"))
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
    // 图片的相对引用放在 data-src 而不是 src：
    // 浏览器不会为缺失 src 的 <img> 发起请求，等前端增强时再解析成 data URL。
    // 否则每个本地图片都会先触发一次注定被 CSP 拦掉的请求（控制台刷错误）。
    let raw = raw.replace("src=\"md-rel://", "data-src=\"md-rel://");
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
        // [TOC] 占位 div 必须保留 class，才能在消毒后替换为真实目录
        .add_tag_attributes("div", &["class"])
        .add_tag_attributes("h1", &["id"])
        .add_tag_attributes("h2", &["id"])
        .add_tag_attributes("h3", &["id"])
        .add_tag_attributes("h4", &["id"])
        .add_tag_attributes("h5", &["id"])
        .add_tag_attributes("h6", &["id"])
        .add_tag_attributes("a", &["id", "class", "href"])
        .add_tag_attributes("input", &["type", "checked", "disabled"])
        .add_tag_attributes("img", &["width", "height", "alt", "title", "data-src"])
        .url_schemes(
            ["http", "https", "mailto", "data", "md-rel", "md-link", "md-toc"]
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
        let r = render_document("# Hello World\n\n## 第二节\n\ntext\n");
        assert!(r.chunks.iter().any(|c| c.contains("id=\"hello-world\"")));
        assert_eq!(r.title.as_deref(), Some("Hello World"));
        assert!(r.toc.iter().any(|t| t.text == "第二节" && t.level == 2));
    }

    #[test]
    fn chunks_split_on_h2() {
        let md = "# A\n\nintro\n\n## One\n\n1\n\n## Two\n\n2\n";
        let r = render_document(md);
        assert_eq!(r.chunks.len(), 3);
    }

    /// A8：blockquote 内的 H2 不得切块（否则容器被切成两半）
    #[test]
    fn heading_inside_blockquote_does_not_split() {
        let md = "> ## 引用内标题\n>\n> 引用正文\n\n## 正常标题\n\n正文\n";
        let r = render_document(md);
        assert_eq!(r.chunks.len(), 2, "只应在顶层 H2 处切一次");
        assert!(r.chunks[0].contains("引用内标题"));
        assert!(r.chunks[1].contains("正常标题"));
    }

    #[test]
    fn toc_records_chunk_index() {
        // 首个 H2 之前的导言自成一章（chunk 0），所以 A 落在 chunk 1
        let md = "intro\n\n## A\n\na\n\n## B\n\nb\n";
        let r = render_document(md);
        for item in &r.toc {
            let chunk = &r.chunks[item.chunk];
            assert!(
                chunk.contains(&format!("id=\"{}\"", item.id)),
                "TOC「{}」指向的块不含该标题",
                item.text
            );
        }
        assert_eq!(r.toc.iter().find(|t| t.text == "A").unwrap().chunk, 1);
        assert_eq!(r.toc.iter().find(|t| t.text == "B").unwrap().chunk, 2);
    }

    #[test]
    fn strips_xss() {
        let md = "safe\n\n<script>alert(1)</script>\n\n<img src=x onerror=\"alert(1)\">\n";
        let r = render_document(md);
        let joined = r.chunks.join("");
        assert!(!joined.contains("<script"));
        assert!(!joined.contains("onerror"));
    }

    #[test]
    fn keeps_tasklist_checkbox() {
        let md = "- [x] done\n- [ ] todo\n";
        let r = render_document(md);
        let joined = r.chunks.join("");
        assert!(joined.contains("type=\"checkbox\""));
    }

    #[test]
    fn parses_frontmatter_title_date_tags() {
        let md = "---\ntitle: 扩散模型讲义\ndate: 2026-08-01\ntags: [AI, 生成模型, 数学]\n---\n\n# 正文标题\n\n内容";
        let r = render_document(md);
        let fm = r.frontmatter.clone().expect("应有 front matter");
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
        let r = render_document(md);
        let fm = r.frontmatter.expect("应有 front matter");
        assert_eq!(fm.tags, vec!["a", "b", "c"]);
    }

    #[test]
    fn parses_frontmatter_comma_tags_and_quotes() {
        let md = "---\ntitle: \"带引号标题\"\ndate: '2026-08-02'\ntags: x, y, z\n---\n\n# H\n";
        let r = render_document(md);
        let fm = r.frontmatter.expect("应有 front matter");
        assert_eq!(fm.title.as_deref(), Some("带引号标题"));
        assert_eq!(fm.date.as_deref(), Some("2026-08-02"));
        assert_eq!(fm.tags, vec!["x", "y", "z"]);
    }

    #[test]
    fn no_frontmatter_when_absent() {
        let md = "# 只有标题\n\n正文";
        let r = render_document(md);
        assert!(r.frontmatter.is_none(), "无 front matter 应为 None");
    }

    /// A1：相对图片统一改写成 md-rel:// 哨兵，外链与 data URL 原样保留
    #[test]
    fn rewrites_relative_images() {
        let md = "![a](./img/a.png)\n\n![b](https://x.com/b.png)\n\n![c](data:image/png;base64,AA)\n";
        let r = render_document(md);
        let joined = r.chunks.join("");
        assert!(joined.contains("md-rel://./img/a.png"), "实际: {joined}");
        assert!(joined.contains("https://x.com/b.png"));
        assert!(joined.contains("data:image/png;base64,AA"));
    }

    #[test]
    fn marks_internal_md_links() {
        let md = "[见架构](./arch.md) and [web](https://x.com)\n";
        let r = render_document(md);
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

    /// A5：索引抽取不生成 HTML，标题优先取 front matter
    #[test]
    fn extract_index_prefers_frontmatter_title() {
        let md = "---\ntitle: 元数据标题\n---\n\n# 正文标题\n\n一些内容\n";
        let d = extract_index(md);
        assert_eq!(d.title.as_deref(), Some("元数据标题"));
        assert!(d.plain.contains("正文标题"));
        assert!(d.plain.contains("一些内容"));
        assert!(!d.plain.contains("---"));
    }

    #[test]
    fn extract_index_falls_back_to_h1() {
        let d = extract_index("# 一级标题\n\n正文\n");
        assert_eq!(d.title.as_deref(), Some("一级标题"));
    }

    #[test]
    fn math_inline_renders_math_span() {
        let md = "行内公式 $E = mc^2$ 结束";
        let out = render_document(md);
        let html: String = out.chunks.concat();
        assert!(
            html.contains("math-inline") || html.contains("class=\"math\""),
            "期望 math span，实际: {html}"
        );
    }

    #[test]
    fn math_block_renders_display_span() {
        let md = "$$\n\\int_0^1 x dx\n$$";
        let out = render_document(md);
        let html: String = out.chunks.concat();
        assert!(
            html.contains("math-display") || html.contains("class=\"math\""),
            "期望 math display span，实际: {html}"
        );
    }

    /// `[TOC]` 生成真实目录，并带 md-toc:// 哨兵
    #[test]
    fn toc_placeholder_is_replaced() {
        let md = "# 标题\n\n[TOC]\n\n## 小节\n\n内容\n";
        let r = render_document(md);
        let joined = r.chunks.join("");
        assert!(joined.contains("md-toc://"), "实际: {joined}");
        assert!(!joined.contains("lnkw-toc-insert"));
    }

    /// A9：围栏代码块内的 [TOC] 必须原样保留
    #[test]
    fn toc_inside_fence_is_preserved() {
        let md = "示例：\n\n```bash\n[TOC]\n```\n\n## 小节\n\n正文\n";
        let r = render_document(md);
        let joined = r.chunks.join("");
        assert!(joined.contains("[TOC]"), "代码块里的 [TOC] 应原样保留: {joined}");
        assert!(!joined.contains("lnkw-toc-insert"));
        assert!(!joined.contains("md-toc://"), "围栏内的 [TOC] 不应生成目录");
    }

    /// A9：围栏内的孤立 $$ 不得翻转全局 in_math 状态
    #[test]
    fn fence_dollar_does_not_leak_math_state() {
        let md = "```text\n$$\n```\n\n甲\n-\n\n乙\n";
        let out = preprocess(md);
        // 围栏外的 `-` 行不应被合并进上一行
        assert!(out.contains("甲\n-\n"), "实际: {out}");
    }
}
