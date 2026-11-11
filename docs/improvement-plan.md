# Lnkwell 架构改进计划

> 版本：v1.0 · 2026-10-18
> 来源：对当前代码库（commit `6d25c80`）的架构评审
> 范围：只做「让已有功能真正可用 + 消除结构性问题」，不新增产品功能

---

## 0. 评审结论摘要

架构主干正确且已落地，不需要推翻重来。真正的问题集中在两处：

1. **平台句柄泄漏**：`entry`（桌面绝对路径 / Android FsUri JSON）被当作普通字符串交给前端做路径运算，导致 Android 上「相对路径图片」和「md 互跳」两条链路实际不可用。
2. **依赖顺序倒置**：搜索索引 → `docs` 行 → 阅读进度/最近阅读，形成「想用进度必须先建索引、想建索引必须先搜索失败」的死循环。

本计划按「先修可用性，再修结构，最后扫尾」推进。

### 0.1 改进原则

- **P1 不引入新依赖能解决的，就不引新依赖。** 索引写库的竞态用「前端驱动循环」解决，比在 Rust 侧再挂一套 SQLite 更省。
- **P2 平台差异只保留一份实现。** 路径解析一律回到 Rust，前端只传不透明句柄。
- **P3 每一处修复都必须有一条可自动运行的断言**（Rust 单测 / 集成测试），Android 专属路径至少在文档里留真机验收项。
- **P4 不改数据契约时优先改代码，改契约必须带迁移。**

### 0.2 任务总览

编号沿用评审报告，`R#` 为原编号，便于对照。

| ID | R# | 问题 | 优先级 | 阶段 | 状态 |
|---|---|---|---|---|---|
| A1 | R1,R2 | 相对路径解析收敛到 Rust（图片 + md 互跳） | P0 | 一 | ✅ 完成 |
| A2 | R3 | `docs` 行按需创建，进度/最近阅读恢复 | P0 | 一 | ✅ 完成 |
| A3 | R4 | 打开文档库后自动建索引 | P0 | 一 | ✅ 完成 |
| A4 | R5,R13 | 索引改前端驱动，修竞态 + 天然背压 | P0 | 一 | ✅ 完成 |
| A5 | R12 | 索引不再为取纯文本做完整渲染 | P0 | 一 | ✅ 完成 |
| A6 | R8 | FTS5 MATCH 查询串转义 | P0 | 一 | ✅ 完成（`scripts/verify-fts.py` 验证） |
| A7 | R9 | 索引对账，清理已删除文件 | P0 | 一 | ✅ 完成 |
| A8 | R6 | 分块边界只在顶层切 | P0 | 一 | ✅ 完成 |
| A9 | R7 | 预处理感知围栏代码块 | P0 | 一 | ✅ 完成 |
| A10 | R10 | 外链交给系统浏览器（opener 插件） | P0 | 一 | ✅ 完成 |
| B1 | R11 | 拆分「阅读渲染」与「索引抽取」两条管线 | P1 | 二 | ✅ 完成 |
| B2 | R17 | 阻塞 IO 移出 async worker | P1 | 二 | ✅ 完成 |
| B3 | R18 | 命令路径去 `unwrap()` | P1 | 二 | ✅ 完成 |
| B4 | R15,R16 | DB 迁移机制 + 删除死表 | P1 | 二 | ✅ 完成 |
| B5 | R14 | 持久层失败显式化 | P1 | 二 | ✅ 完成 |
| B6 | R20 | 文档切换竞态（keyed 重挂载） | P1 | 二 | ✅ 完成 |
| C1 | R21 | 清理死依赖与多余权限 | P2 | 三 | ✅ 完成 |
| C2 | R19 | `rel_path` 语义与展示修正 | P2 | 三 | ✅ 完成 |
| C3 | R24,R23 | fixtures 回归集 + README | P2 | 三 | ✅ 完成 |
| C4 | R25 | 方案文档与实现同步 | P2 | 三 | ✅ 完成 |
| A11 | 新增 | **FTS5 外部内容表列不匹配（跨文件搜索从未生效）** | P0 | 补做 | ✅ 完成 |
| D1 | — | Android 真机验收清单 | 必须 | 持续 | ⏳ 待真机 |

---

## 1. 阶段一：修可用性（P0）

### A1 · 相对路径解析收敛到 Rust

**现象**
- Android 上所有相对路径图片加载失败。
- Android 上点击 md 互跳链接必然报错（`INVALID_INPUT`）。

**根因**
1. `vault/android.rs::resolve_img`（第 144 行）产出 `md-file://` + 裸 `content://` URL，而 `read_image` 统一用 `FsUri::from_json_str` 解析，裸 URL 不是 JSON。同一文件里的 Android 条目全都是 JSON，契约自相矛盾。
2. `render_document` 的图片解析回调是**同步**的 `&dyn Fn(&str) -> String`，而 SAF 的 URI 解析（`resolve_file_uri`）是 **async** 的 Kotlin 调用 —— 在渲染期根本不可能解析，这是设计层面的死结。
3. `ArticleView.resolveRelativePath` 在前端用 `split('/')` 拼路径，只对桌面绝对路径成立；Rust 里已有同义实现 `vault::resolve_img`，属于第二份实现。

**方案**
1. 渲染期不再解析图片路径。`markdown.rs` 把一切非 `http(s)`/`data:` 的图片改写为哨兵 `md-rel://<原始相对路径>`，与平台无关。
2. 新增命令 `resolve_entry(doc_entry, rel) -> { entry, name }`：唯一的相对路径解析入口。
   - 桌面：`parent(doc).join(rel)` + 组件级 `..` 规范化。
   - Android：用父目录 URI + `AndroidFs::resolve_file_uri(dir, rel)`；父目录 URI 由 `read_dir` 期间维护的「子 URI → 父目录 URI」映射表提供（`list` 与 `collect_markdown` 都会填充）。
3. 新增命令 `read_image_in_doc(doc_entry, rel) -> data URL`：`resolve_entry` + `read_image_bytes` 组合，平台实现只需 `resolve_entry` 一处。
4. 前端 `enhance.ts` 处理 `img[src^="md-rel://"]` 时改为调用 `read_image_in_doc`，不再自己拼路径；`ArticleView` 点击 `md-link://` 时调用 `resolve_entry`。
5. 删除 `vault::resolve_img`（两个平台）与前端 `resolveRelativePath`。

**涉及文件**
`src-tauri/src/markdown.rs`、`src-tauri/src/vault/{mod,desktop,android}.rs`、`src-tauri/src/lib.rs`、`src/lib/api.ts`、`src/lib/render/enhance.ts`、`src/components/ArticleView.svelte`

**验收**
- 桌面：`resolve_entry` 单测覆盖 `./img/a.png`、`../assets/b.png`、`sub/../x/y.png`、`http://` 原样返回。
- 渲染单测：相对图片产出 `md-rel://`，`data:` 与 `https:` 不被改写。
- Android 真机：含 `<img src="img/a.png">` 的文档能显示图；点 `./other.md` 能跳转（列入 D1）。

**风险**
Android 父目录映射在「冷启动后直接从书签打开深层文档」时可能缺失（映射表为空）。缓解：A3 之后打开文档库即全量遍历，映射表会被 `collect_markdown` 填满。缺失时返回 `NOT_FOUND`，图片降级为「加载失败」提示，不崩溃。

---

### A2 · `docs` 行按需创建

**现象**：阅读进度永远不保存；「最近阅读」永远为空。

**根因**：`saveProgress` 依赖 `docs` 行，但只有 `indexDoc` / `addBookmark` 会创建它；而索引在正常路径下从不启动（见 A3）。

**方案**
1. `db.ts` 增加 `ensureDoc(vaultId, entry): Promise<number | null>`，`INSERT ... ON CONFLICT DO NOTHING` + `SELECT id`。
2. `saveProgress`、`addBookmark` 统一走 `ensureDoc`，删除 `addBookmark` 里重复的那段插入逻辑。
3. `ArticleView` 恢复进度时若 `docs` 行不存在，返回 `null` 属正常路径，不再当作异常。

**验收**：不建索引的情况下打开任意文档滚动 → 退出 → 重开，位置恢复；Welcome 出现「最近阅读」。

---

### A3 · 打开文档库后自动建索引

**现象**：新用户搜索永远空；与方案 §8.5「首次选择文档库时后台建索引」相反。

**方案**
- `vault.setRoot()` 成功后触发一次 `void this.startIndexing()`（不 await，不阻塞目录树展示）。
- 已有索引时按 mtime 增量跳过（见 A4）。
- 索引期间底部显示进度条（已有 `.index-banner`）。

**验收**：首次选库后 3 秒内搜索可命中；二次启动不重复解析未变更文件。

---

### A4 · 索引改由前端驱动

**现象**：`build_index` 边 emit 边返回，前端 `void indexDoc(...)` 不 await → 索引「完成」时 DB 写入仍在飞行，立刻搜索会漏；错误被 `void` 吞掉。

**根因**：控制流在 Rust，数据落在 JS，中间靠一次性事件，没有背压也没有完成信号。

**方案**
1. 删除 Rust 命令 `build_index` 与 `index-file` / `index-progress` / `index-done` 三个事件。
2. 保留 `collect_markdown_files`（返回值增加 `mtime`）与 `extract_plain`。
3. 前端 `startIndexing` 改为顺序循环：
   ```ts
   const files = await api.collectMarkdownFiles(root);
   const known = await docMtimes(vaultId);          // Map<entry, mtime>
   for (const [i, f] of files.entries()) {
     if (f.mtime && known.get(f.entry) === f.mtime) { skip++; continue; }
     const p = await api.extractPlain(f.entry);     // 有背压
     await indexDoc(vaultId, p.entry, p.title, p.plain, f.mtime ?? 0);
     progress = { done: i + 1, total: files.length };
   }
   ```
4. 单篇失败 `try/catch` 继续，最后汇总。

**收益**：竞态消失、背压天然、进度真实、错误可见；Rust 少 40 行。

**验收**：千篇库索引结束后立即搜索可全量命中；中途取消（切库）不会串库。

---

### A5 · 索引不再完整渲染

**现象**：`build_index` 为了拿 `plain`，对每篇文档执行完整渲染管线（分块 + 图片重写 + ammonia 消毒），结果全部丢弃。

**方案**：新增 `markdown::extract_index(md) -> IndexDoc { title, plain }`，只做一次 parser 遍历取纯文本 + 标题（front matter 优先，其次首个 H1）。`extract_plain` 命令改用它。

**验收**：索引 100 篇的耗时下降到接近原 1/3（以一次手工计时记录在 D1）。

---

### A6 · FTS5 查询串转义

**现象**：`searchDocs` 把用户输入拼进 `"${query}"`，输入一个双引号即产生非法 FTS 语法并抛错。

**方案**
```ts
function ftsPhrase(q: string): string {
  return `"${q.replace(/"/g, '""')}"`;
}
```
并加最小长度判断（`< 3` 直接返回空，已有）。同时把 `ORDER BY rank` 显式写成 `ORDER BY docs_fts.rank`（JOIN 场景下更稳妥）。

**验收**：输入 `a"b`、`(`、`*` 均不抛错。

---

### A7 · 索引对账

**现象**：删除/重命名的文件永远留在 `docs` / `docs_fts`，搜索结果点开即 `NOT_FOUND`。

**方案**：索引循环结束后调用 `pruneDocs(vaultId, keepEntries)`：先 `SELECT id, rel_path`，在 JS 侧求差集，按 id 分批（每批 500）`DELETE FROM docs WHERE id IN (...)`；`progress` / `bookmarks` / `doc_body` 由既有 `ON DELETE CASCADE` 自动清理。

**验收**：删除一个已索引文件后重建索引，搜索不再命中；其书签自动消失。

---

### A8 · 分块边界只在顶层切

**现象**：`render_document` 见到 `<h2 ` 字符串就切块。若 H2 位于 blockquote / `<details>` 内（`> ## 小节` 很常见），容器被切成两半，两半都是不平衡 HTML，ammonia 各自补闭合 → 引用块提前结束、标题掉出样式。

**方案**：改为在第一遍事件遍历时维护容器深度栈（`Start(BlockQuote|List|Item|FootnoteDefinition|Table*|DefinitionList*|HtmlBlock)` 加深度，对应 `End` 减），**仅当深度为 0 且遇到 H2** 时切块；同时在该处直接把 `chunk` 序号写进 `TocItem`，删除原先靠字符串前缀匹配回填 chunk 的脆弱逻辑。

**验收**
- 单测：`> ## 引用内标题` 不被切成两块；普通 `## ` 仍切分。
- 既有 `chunks_split_on_h2` 等测试保持通过。

---

### A9 · 预处理感知围栏代码块

**现象**（已复现）
- 代码块里的 `[TOC]` 被替换成占位 `div`，随后又被替换成真目录，把示例代码改坏。
- 代码块里的 `$$` 会翻转 `in_math`，影响之后整篇的 `=` / `-` 行合并。

**方案**：`preprocess` 增加围栏状态机（``` 或 ~~~ 开头，长度 ≥ 3，闭合需同字符且长度不短于开启），在围栏内**原样输出**，不做任何替换；仅围栏外处理 `[TOC]`、`$$` 与 `=`/`-` 合并。

**验收**：新增单测 —— 围栏内 `[TOC]` 保持原样、围栏内 `$$` 不影响后续 `-` 行；围栏外行为不变。

---

### A10 · 外链交给系统浏览器

**现象**：`window.open(href, '_blank')` 在 Tauri WebView 中被吞掉，点击外链无反应。

**方案**：接入 `tauri-plugin-opener`（crate + npm + capability `opener:allow-open-url`），`ArticleView` 改调 `openUrl(href)`。

**验收**：桌面点击外链唤起默认浏览器；Android 点击外链唤起系统浏览器。

---

## 2. 阶段二：修结构（P1）

### B1 · 拆分阅读与索引两条管线

`RenderResult` 去掉 `plain` 字段（阅读路径不传全文），索引走 A5 的 `extract_index`。收益：每次打开文档少一次完整 parse + 一次全文 IPC 传输；进程内渲染缓存也不再持有全文。

### B2 · 阻塞 IO 移出 async worker

`desktop::read_text` / `read_image_bytes` 用 `tauri::async_runtime::spawn_blocking` 包裹 `std::fs` 读取与解码；`read_doc` / `extract_plain` 中的 `render_document` 同样 `spawn_blocking`（它是纯 CPU 计算，会把 tokio worker 占满）。

### B3 · 命令路径去 `unwrap()`

`pending_share()` / `render_cache()` 的 `lock().unwrap()` 改为 `lock().unwrap_or_else(|e| e.into_inner())`（锁中毒时继续用内部值，而不是 panic）。与方案 §9.5 的硬性约定对齐。

### B4 · DB 迁移机制 + 删死表

1. 引入 `PRAGMA user_version` + 顺序迁移数组，替代「只有 `CREATE TABLE IF NOT EXISTS`」。
2. 迁移 v2：`DROP TABLE IF EXISTS render_cache`（全库无人读写的死表）。
3. 迁移 v2 顺带补齐 `bookmarks(doc_id, anchor)` 唯一索引 —— 先删重复行再建索引，路径可预期。
4. 删除手写 SQL 分句器 `splitSchema`：schema 改为语句数组，直接逐条执行。

### B5 · 持久层失败显式化

`getDb()` 不再「静默返回 null」：暴露 `dbStatus`（`ok | unavailable`）+ 失败原因；启动时若不可用，toast 一次「本地数据库不可用，检索/进度/书签已关闭」。各调用点仍保持「无库不崩」的行为，但用户第一次就能知道。

### B6 · 文档切换竞态

`App.svelte` 中用 `{#key nav.current.entry}` 包住 `ArticleView`，切换文档时组件销毁重建，彻底消除旧文档的 async 尾段写入新 DOM 的问题（`renderedCount`、`enhancedBoundary`、进度滚动均随组件销毁失效）。

---

## 3. 阶段三：扫尾（P2）

### C1 · 清理死依赖与多余权限
- 删除 Rust `tauri-plugin-fs` 依赖与插件注册（文件访问全走自定义命令）。
- 删除 capability 中的 `fs:default`。
- 删除前端未使用的 `@tauri-apps/plugin-fs`、`@tauri-apps/plugin-dialog`。
- 关闭 `assetProtocol`（图片走 data URL，`scope: ["**"]` 是纯风险）；保留 `protocol-asset` feature 则一并去掉。

### C2 · `rel_path` 语义与展示
- DB 列名迁移为 `entry`（迁移 v3），所有查询同步。
- 列表/书签/搜索结果的路径展示改用「去除库根后的相对路径」或仅文件名，避免 Android 上显示整段 JSON。

### C3 · fixtures 回归集 + README
- `fixtures/cases/` 增加：CRLF、XSS、围栏内 `[TOC]`、blockquote 内 H2、深层嵌套、`../` 图片、万行长文（生成脚本）。
- 新增 `src-tauri/tests/render_regression.rs` 驱动这些 fixture 做快照断言。
- 补 `README.md`：定位、技术栈、开发命令、目录结构、已知限制。

### C4 · 方案文档同步
在技术方案文档追加「附录 D：实现状态与偏差」，记录：KaTeX 版本（`^0.16` 而非 0.18）、SQLite 在 TS 侧、capabilities 已无 `android-fs:default`、图片走 data URL 而非 asset 协议、`exportStyle.ts` 注释与实际 `media="print"` 的矛盾修正。

---

## 4. 执行顺序与依赖

```
A1 ─┬─> A2 ─> A3 ─> A4 ─> A5 ─> A7
    │                 └─> A6
    └─> A8 ─> A9 ─> A10
B1 依赖 A5；B2/B3 独立；B4 依赖 A2（ensureDoc 已建）；B5/B6 独立
C1/C2 依赖 B4（迁移）；C3/C4 最后
```

每完成一项：`cargo test` 必须全绿；涉及前端时 `npm run check` 必须无新增错误。

---

## 5. 验收与回归

| 层 | 命令 | 覆盖 |
|---|---|---|
| Rust 单测 | `cd src-tauri && cargo test` | 渲染、编码、路径解析、分块、预处理、消毒 |
| Rust 集成 | `cargo test --test render_regression` | fixtures 快照回归 |
| 前端类型 | `npm run check` | Svelte 5 runes + TS 契约一致 |
| 前端构建 | `npm run build` | 产物可打包 |
| 真机 | D1 清单 | Android SAF、图片、互跳、外链 |

## 6. D1 · Android 真机验收清单

- [ ] 选库 → 目录树可展开 → md 可预览
- [ ] 文档内相对图片正常显示（含 `../assets/x.png`）
- [ ] 点击 `./other.md` 能跳转，返回键可回退
- [ ] 杀掉进程后重进：SAF 权限仍有效（或提示重选）
- [ ] 分享单个 md 进来能打开（临时文档，无相对图片）
- [ ] 外链唤起系统浏览器

## 7. 明确不做（本轮）

- 编辑能力（产品定位）
- 跨会话渲染缓存（`render_cache` 表删除，改用内存缓存）
- 桌面端改用 asset 协议流式加载图片（保留 data URL，后续评估）
- 前端单测框架（Vitest）搭建 —— 留待 C3 之后单独排期
- 多库同开（schema 已留位，UI 不做）

---

## 8. 执行记录（2026-10-18）

### 8.1 验证结果

| 检查 | 结果 |
|---|---|
| `cargo test`（桌面） | **57 passed, 0 failed**（单测 27 + 集成 30，共 12 个测试二进制） |
| `cargo check --target aarch64-linux-android --lib` | **通过**（Android 专属代码已做类型检查） |
| `npm run check` | **0 errors / 0 warnings** |
| `npm run build` | 成功（约 5.5s） |
| `python scripts/verify-fts.py` | **24 项断言全过**（含老库迁移与旧 bug 复现） |

### 8.2 与计划的偏差（实现时的调整）

1. **A1**：`resolve_entry` 只返回条目字符串，不返回 `{entry, name}` —— 文件名由相对路径末段即可得到，不必让后端参与展示逻辑。
2. **A1**：`render_document` 彻底去掉解析回调参数。既然解析期不碰平台路径，回调就没有存在意义；这同时消除了「同步回调里调 async SAF」的死结。
3. **A4**：保留 `collect_markdown_files`，但给它加上 `mtime`（`FileNode.mtime`）以支持增量；删除 `build_index` 命令与 `index-file` / `index-progress` / `index-done` 三个事件。
4. **A7**：对账时**先删 `doc_body` 再删 `docs`**。若直接删 `docs`，级联删除触发 FTS 外部内容表的删除触发器时父行已不可见，`(SELECT title FROM docs ...)` 读到 NULL，索引会残留。
5. **B4/C2**：迁移从「语句数组」改为「版本 + 运行函数」。因为 C2 的列重命名必须先查 `PRAGMA table_info` 判断是否已改名，否则二次执行报错。
6. **C2**：v1 schema 直接写 `entry` 列，迁移 v3 负责把老库的 `rel_path` 改名。新库不需要经过中间态，老库仍可升级。
7. **A8/A9 合并实现**：分块与预处理都在 `markdown.rs`，一次重构完成，避免两次改动同一函数。
8. **集成测试整改（计划外）**：发现 7 个既有集成测试硬编码了 `D:\code\扩散模型\扩散模型统一讲义.md`，在任何其它机器上必然失败。已全部改为「仓库内 fixture + 合成文档」，并抽出 `tests/common/mod.rs` 共享助手。

### 8.3 新增测试

| 文件 | 覆盖 |
|---|---|
| `src-tauri/src/markdown.rs`（单测） | 顶层分块、blockquote 内 H2 不切块、围栏内 `[TOC]`/`$$`、索引抽取优先级、`md-rel://` 改写 |
| `src-tauri/tests/render_regression.rs` | 8 个 fixture 的消毒/分块平衡/TOC 指向/确定性/长文档线性 |
| `src-tauri/tests/real_doc.rs` · `img_pipeline.rs` | 哨兵 → 解析 → 读字节 → data URL 往返；中文与含空格路径的可逆性 |
| `src-tauri/tests/toc_*.rs` · `real_toc*.rs` | TOC 锚点一致性、chunk 序号合法性、大文档嵌套结构 |

### 8.4 追加发现：跨文件搜索从未生效（A11）

在给 A6 补实测时发现，实际问题比「转义缺失」严重得多。

**根因**：`docs_fts` 是 **external content** 表（`content='doc_body'`），FTS5 要求它声明的
每个列名都能在内容表 `doc_body` 里找到同名真实列。但 `title` 只存在 `docs` 表，
`doc_body` 只有 `doc_id, body` —— 于是 SQLite 组装外部内容查询时得到
`no such column: T.title`，`snippet()` 直接抛 `SQL logic error`。

`searchDocs()` 每次都会调用 `snippet()`，所以它**每次都抛错**；
`SearchPanel` 把它 catch 成一条 toast，用户看到的是「没有结果」。
这也解释了为什么没人报告「FTS 转义问题」——那条代码路径根本没跑通过。

**复现**（`scripts/verify-fts.py` 用例 7 保留了这个回归）：

```
旧结构: docs_fts(title, body) + doc_body(doc_id, body)
  SELECT snippet(docs_fts, 1, ...) WHERE MATCH ...  ->  SQL logic error
```

**修复**：
1. `doc_body` 增加 `title` 列，触发器直接取 `new.title` / `old.title`，
   不再跨表子查询（顺带消除了「级联删除时父行不可见」的隐患）。
2. `indexDoc` 同时写入 `docs.title` 与 `doc_body.title`。
3. 新增迁移 **v4**：拆除旧 FTS 与触发器 → `ALTER TABLE doc_body ADD COLUMN title`
   → 从 `docs` 回填 → 按新结构重建 → `INSERT INTO docs_fts(docs_fts) VALUES('rebuild')`。
4. `snippet(docs_fts, 1, ...)` 的列号保持不变（0=title, 1=body）。

**验证**：`scripts/verify-fts.py` 在 Python 自带 SQLite 上镜像同构 schema，24 项断言全过，
覆盖：索引可读、中文子串、标题检索、转义、触发器三向同步、对账无残留、老库迁移。

### 8.5 遗留事项

1. **D1 真机验收**：Android 侧的图片/互跳已通过 `cargo check --target aarch64-linux-android`，
   但 SAF 的 `resolve_file_uri` 行为与父目录映射的完整性仍须真机确认。
2. **`scripts/verify-fts.py` 未进 CI**：它依赖 Python，暂为手动/本地脚本。
   接入 Vitest 后可把等价的 SQL 断言搬进前端测试。
3. **`app-icon-candidates/` 已归档**到 `docs/assets/icon-candidates/`，如不再需要可直接删除。
4. **`ORDER BY docs_fts.rank` 已验证可用**（脚本用例 2），无需改为 `bm25()`。

---

## 9. 构建记录：Windows 打包（2026-10-18）

首次成功产出 Windows 安装包。过程中修复了 3 个**既有**构建阻塞（与 §8 的代码改动无关，
它们在本次构建前就存在——也就是说仓库此前无法通过 `tauri build`）。

| # | 现象 | 根因 | 修复 |
|---|---|---|---|
| 1 | `tauri build` 预检直接报 `version mismatched`，不进入编译 | `@tauri-apps/api` 2.12.4 vs `tauri` crate 2.11.5；`@tauri-apps/plugin-opener` 2.7.2 vs crate 2.5.5 | npm 侧精确锁到同 minor：`@tauri-apps/api@2.11.1`、`@tauri-apps/plugin-opener@2.5.5` |
| 2 | `tauri-plugin-deep-link` 构建脚本 panic：`invalid type: string "content", expected a sequence` | `plugins.deep-link.mobile[].scheme` 在插件 schema 里是 `Vec<String>`，配置写成了字符串；且用 `content` 当 scheme 等于把应用注册成所有 `content://` URI 的处理器 | 移除该配置；Android 分享接收按方案 §7.4 手写 `AndroidManifest` intent-filter |
| 3 | MSI 打包时 `light.exe` 报 `LGHT0311`（代码页 1252 装不下非 Latin 字符） | `bundle.fileAssociations[].description` 是中文，而 WiX 默认 en-US 使用 cp1252 | `tauri.conf.json` 设 `bundle.windows.wix.language = "zh-CN"`（cp936） |

补充：NSIS 工具链需首次从 GitHub 下载，而 Tauri CLI 不读 Windows 系统代理设置，
必须显式提供 `HTTP_PROXY` / `HTTPS_PROXY` 环境变量。

### 9.1 产物

| 产物 | 大小 |
|---|---|
| `src-tauri/target/release/lnkwell.exe`（便携版） | 14.85 MB |
| `src-tauri/target/release/bundle/nsis/Lnkwell_0.1.0_x64-setup.exe` | 5.28 MB |
| `src-tauri/target/release/bundle/msi/Lnkwell_0.1.0_x64_zh-CN.msi` | 6.83 MB |

### 9.2 启动冒烟测试（release exe，本次实跑）

- 进程启动后存活 8 秒未崩溃，内存 39.7 MB / 45 线程，窗口可正常关闭；
- 首次启动即成功建库，落在 `%APPDATA%\com.lnkwell.app\lnkwell.db`；
- 核对库结构：`PRAGMA user_version = 4`、`docs(entry, …)`（v3 改名生效）、
  `doc_body(doc_id, title, body)`（v4 补列生效）、无 `render_cache`（v2 删除生效）、
  `idx_bookmarks_doc_anchor` 存在（v2 建立）、`docs_fts` 上的 `snippet()` 检索 SQL 可正常执行
  （A11 修复在真实库上生效）。

> 结论：迁移链 v1→v4 与检索层修复在真实应用中均已验证，不只是单测/脚本层面。
