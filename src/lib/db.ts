import Database from '@tauri-apps/plugin-sql';

/**
 * 数据层（方案 §7.3）：v1 即带 vault_id，FTS5 用 external content + trigram，
 * 保证中文子串可搜、snippet() 可生成命中摘要。
 */

let db: Database | null = null;

export interface VaultRow {
  id: number;
  name: string;
  root_uri: string;
  last_opened_at: number | null;
}

export interface SearchHit {
  doc_id: number;
  entry: string;
  title: string | null;
  excerpt: string;
}

export interface ProgressRow {
  anchor: string | null;
  section_offset: number | null;
}

export interface RecentDoc {
  entry: string;
  title: string | null;
  excerpt: string | null;
  updated_at: number;
  vault_root: string;
}

/**
 * FTS5 全文索引与同步触发器。
 *
 * ⚠️ 关键约束：`content='doc_body'` 是**外部内容表**，FTS 声明的每个列名
 * 都必须在 `doc_body` 里存在同名真实列。此前 `title` 只存在 `docs` 表里，
 * 导致 `snippet()` 读取外部内容时直接抛 `SQL logic error` ——
 * 跨文件搜索每次都在抛错（只是被 try/catch 吞成一条 toast）。
 *
 * 因此 `doc_body` 同时存 title 与 body，触发器直接取 `new.title`，
 * 不再跨表子查询（也顺带避免了级联删除时父行不可见的问题）。
 */
const FTS_SCHEMA: string[] = [
  `CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
    title,
    body,
    content='doc_body',
    content_rowid='doc_id',
    tokenize='trigram'
  )`,
  `CREATE TRIGGER IF NOT EXISTS doc_body_ai AFTER INSERT ON doc_body BEGIN
    INSERT INTO docs_fts(rowid, title, body)
      VALUES (new.doc_id, new.title, new.body);
  END`,
  `CREATE TRIGGER IF NOT EXISTS doc_body_ad AFTER DELETE ON doc_body BEGIN
    INSERT INTO docs_fts(docs_fts, rowid, title, body)
      VALUES('delete', old.doc_id, old.title, old.body);
  END`,
  `CREATE TRIGGER IF NOT EXISTS doc_body_au AFTER UPDATE ON doc_body BEGIN
    INSERT INTO docs_fts(docs_fts, rowid, title, body)
      VALUES('delete', old.doc_id, old.title, old.body);
    INSERT INTO docs_fts(rowid, title, body)
      VALUES (new.doc_id, new.title, new.body);
  END`,
];

/**
 * v1 建表语句（改进计划 B4：改为「语句数组 + user_version 迁移」）。
 * 每条语句独立执行：SQLite 的 CREATE TRIGGER 体内含分号，
 * 整段 SQL 一起执行会被切成残缺语句（旧代码为此手写了 splitSchema 分句器，已废弃）。
 */
const SCHEMA_V1: string[] = [
  `CREATE TABLE IF NOT EXISTS vaults (
    id              INTEGER PRIMARY KEY,
    name            TEXT    NOT NULL,
    root_uri        TEXT    NOT NULL UNIQUE,
    last_opened_at  INTEGER
  )`,
  `CREATE TABLE IF NOT EXISTS docs (
    id              INTEGER PRIMARY KEY,
    vault_id        INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
    entry           TEXT    NOT NULL,
    title           TEXT,
    mtime           INTEGER,
    UNIQUE(vault_id, entry)
  )`,
  `CREATE TABLE IF NOT EXISTS progress (
    doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
    anchor          TEXT,
    section_offset  REAL,
    updated_at      INTEGER
  )`,
  `CREATE TABLE IF NOT EXISTS bookmarks (
    id              INTEGER PRIMARY KEY,
    doc_id          INTEGER NOT NULL REFERENCES docs(id) ON DELETE CASCADE,
    anchor          TEXT    NOT NULL,
    note            TEXT,
    created_at      INTEGER
  )`,
  // title 必须与 docs_fts 的列对应（见 FTS_SCHEMA 的说明）
  `CREATE TABLE IF NOT EXISTS doc_body (
    doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
    title           TEXT,
    body            TEXT    NOT NULL
  )`,
  ...FTS_SCHEMA,
  `CREATE TABLE IF NOT EXISTS recent (
    doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
    updated_at      INTEGER NOT NULL
  )`,
];

/**
 * 一条迁移。用函数而非语句数组，是为了支持「有条件的 DDL」
 * （例如重命名列前必须先查 PRAGMA table_info，否则二次执行会报错）。
 */
interface Migration {
  version: number;
  run: (d: Database) => Promise<void>;
}

async function runStatements(d: Database, statements: string[]): Promise<void> {
  for (const stmt of statements) {
    await d.execute(stmt);
  }
}

/**
 * 迁移列表：每条必须**幂等** —— 读取 user_version 失败时会从 1 重跑。
 */
const MIGRATIONS: Migration[] = [
  { version: 1, run: (d) => runStatements(d, SCHEMA_V1) },
  {
    version: 2,
    run: (d) =>
      runStatements(d, [
        // 死表：全库无人读写（渲染缓存在 Rust 进程内）
        'DROP TABLE IF EXISTS render_cache',
        // 同一文档同一锚点只应有一条书签，去重后补唯一索引
        `DELETE FROM bookmarks WHERE id NOT IN (
          SELECT MIN(id) FROM bookmarks GROUP BY doc_id, anchor
        )`,
        'CREATE UNIQUE INDEX IF NOT EXISTS idx_bookmarks_doc_anchor ON bookmarks(doc_id, anchor)',
      ]),
  },
  {
    // 改进计划 C2：`rel_path` 存的一直是完整 entry（桌面绝对路径 / Android FsUri JSON），
    // 名不副实，且误导了 UI 直接拿它当路径展示。
    version: 3,
    run: async (d) => {
      let hasOldColumn = true;
      try {
        const cols = await d.select<{ name: string }[]>('PRAGMA table_info(docs)');
        hasOldColumn = cols.some((c) => c.name === 'rel_path');
      } catch {
        // 拿不到表结构时不擅自改名，交由后续查询报错暴露（比"猜"更安全）
        hasOldColumn = false;
      }
      if (!hasOldColumn) return;
      await d.execute('ALTER TABLE docs RENAME COLUMN rel_path TO entry');
    },
  },
  {
    // 修复 FTS5 外部内容表列不匹配（详见 FTS_SCHEMA 注释）。
    // 老库里 docs_fts(title, body) 声明的 title 在 doc_body 中不存在，
    // snippet() 必然抛错 —— 表现为"跨文件搜索永远没结果"。
    version: 4,
    run: async (d) => {
      let hasTitle = false;
      try {
        const cols = await d.select<{ name: string }[]>('PRAGMA table_info(doc_body)');
        hasTitle = cols.some((c) => c.name === 'title');
      } catch {
        hasTitle = false;
      }
      if (hasTitle) return;

      // 1) 拆掉旧索引与触发器（它们引用了不存在的列）
      await d.execute('DROP TRIGGER IF EXISTS doc_body_ai');
      await d.execute('DROP TRIGGER IF EXISTS doc_body_ad');
      await d.execute('DROP TRIGGER IF EXISTS doc_body_au');
      await d.execute('DROP TABLE IF EXISTS docs_fts');
      // 2) doc_body 补 title 列，并从 docs 回填
      await d.execute('ALTER TABLE doc_body ADD COLUMN title TEXT');
      await d.execute(
        'UPDATE doc_body SET title = (SELECT title FROM docs WHERE id = doc_body.doc_id)'
      );
      // 3) 按新结构重建索引与触发器
      await runStatements(d, FTS_SCHEMA);
      // 4) 用外部内容表重建全文索引
      await d.execute("INSERT INTO docs_fts(docs_fts) VALUES('rebuild')");
    },
  },
];

/** 数据库不可用时的原因，供 UI 一次性提示（改进计划 B5） */
let dbFailure: string | null = null;

/** 持久层状态：调用过 getDb() 后才有意义 */
export function dbStatus(): { available: boolean; error: string | null } {
  return { available: db != null, error: dbFailure };
}

/** 读取 PRAGMA user_version；失败返回 0（迁移均为幂等，重跑安全） */
async function schemaVersion(d: Database): Promise<number> {
  try {
    const rows = await d.select<{ user_version: number }[]>('PRAGMA user_version');
    return rows[0]?.user_version ?? 0;
  } catch {
    return 0;
  }
}

async function migrate(d: Database): Promise<void> {
  let version = await schemaVersion(d);
  for (const m of MIGRATIONS) {
    if (m.version <= version) continue;
    await m.run(d);
    // 版本号只是"下次别再跑"的优化：写失败时退化为每次启动重跑（迁移均幂等），
    // 绝不能因此把整个数据库判为不可用。
    try {
      // PRAGMA 不支持参数绑定，这里插值的是代码内常量
      await d.execute(`PRAGMA user_version = ${m.version}`);
    } catch (e) {
      console.warn('[db] 写入 user_version 失败，迁移将在下次启动重跑', e);
    }
    version = m.version;
  }
}

export async function getDb(): Promise<Database | null> {
  if (db) return db;
  try {
    const d = await Database.load('sqlite:lnkwell.db');
    await migrate(d);
    db = d;
    return d;
  } catch (e) {
    // 显式记录失败原因，不再只打一行 console.warn（改进计划 B5）
    dbFailure = e instanceof Error ? e.message : String(e);
    console.warn('sqlite unavailable', e);
    return null;
  }
}

export async function upsertVault(name: string, rootUri: string): Promise<VaultRow | null> {
  const d = await getDb();
  if (!d) return null;
  await d.execute(
    'INSERT INTO vaults (name, root_uri, last_opened_at) VALUES (?, ?, ?) \
     ON CONFLICT(root_uri) DO UPDATE SET name = excluded.name',
    [name, rootUri, Date.now()]
  );
  const rows = await d.select<VaultRow[]>('SELECT * FROM vaults WHERE root_uri = ?', [rootUri]);
  return rows[0] ?? null;
}

export async function touchVault(id: number): Promise<void> {
  const d = await getDb();
  await d?.execute('UPDATE vaults SET last_opened_at = ? WHERE id = ?', [Date.now(), id]);
}

export async function lastVault(): Promise<VaultRow | null> {
  const d = await getDb();
  if (!d) return null;
  const rows = await d.select<VaultRow[]>(
    'SELECT * FROM vaults ORDER BY last_opened_at DESC LIMIT 1'
  );
  return rows[0] ?? null;
}

/** 索引单篇文档（方案 §8.5：external content 表靠触发器同步 FTS） */
export async function indexDoc(
  vaultId: number,
  relPath: string,
  title: string | null,
  plain: string,
  mtime: number
): Promise<void> {
  const d = await getDb();
  if (!d) return;
  await d.execute(
    'INSERT INTO docs (vault_id, entry, title, mtime) VALUES (?, ?, ?, ?) \
     ON CONFLICT(vault_id, entry) DO UPDATE SET title = excluded.title, mtime = excluded.mtime',
    [vaultId, relPath, title, mtime]
  );
  const rows = await d.select<{ id: number }[]>(
    'SELECT id FROM docs WHERE vault_id = ? AND entry = ?',
    [vaultId, relPath]
  );
  const docId = rows[0]?.id;
  if (docId == null) return;
  // title 同时写进 doc_body：FTS5 外部内容表要求列名与 content 表一致（见 FTS_SCHEMA）
  await d.execute(
    'INSERT INTO doc_body (doc_id, title, body) VALUES (?, ?, ?) \
     ON CONFLICT(doc_id) DO UPDATE SET title = excluded.title, body = excluded.body',
    [docId, title, plain]
  );
}

/**
 * 确保 docs 表存在该条目，返回 doc_id（改进计划 A2）。
 *
 * 进度、书签、索引都以 doc_id 为外键，必须先有 docs 行。旧实现只有索引会创建它，
 * 于是「不建索引 = 阅读进度与书签静默失效」——用户看不到任何错误，只是功能没反应。
 */
export async function ensureDoc(vaultId: number, entry: string): Promise<number | null> {
  const d = await getDb();
  if (!d) return null;
  await d.execute(
    'INSERT INTO docs (vault_id, entry, title, mtime) VALUES (?, ?, ?, ?) \
     ON CONFLICT(vault_id, entry) DO NOTHING',
    [vaultId, entry, null, 0]
  );
  const rows = await d.select<{ id: number }[]>(
    'SELECT id FROM docs WHERE vault_id = ? AND entry = ?',
    [vaultId, entry]
  );
  return rows[0]?.id ?? null;
}

/** 已索引条目的 mtime 快照，用于增量索引（改进计划 A4） */
export async function docMtimes(vaultId: number): Promise<Map<string, number>> {
  const map = new Map<string, number>();
  const d = await getDb();
  if (!d) return map;
  const rows = await d.select<{ entry: string; mtime: number | null }[]>(
    'SELECT entry, mtime FROM docs WHERE vault_id = ?',
    [vaultId]
  );
  for (const r of rows) {
    if (r.mtime != null) map.set(r.entry, r.mtime);
  }
  return map;
}

/**
 * 清理库中已不存在的条目（改进计划 A7）。
 *
 * 先删 doc_body 再删 docs：FTS 的 external content 删除触发器需要从 docs 里查 title，
 * 若直接删 docs，级联删除时父行已不可见，title 会读成 NULL 导致索引残留。
 * progress / bookmarks 由 ON DELETE CASCADE 自动清理。
 */
export async function pruneDocs(vaultId: number, keep: Set<string>): Promise<number> {
  const d = await getDb();
  if (!d) return 0;
  const rows = await d.select<{ id: number; entry: string }[]>(
    'SELECT id, entry FROM docs WHERE vault_id = ?',
    [vaultId]
  );
  const stale = rows.filter((r) => !keep.has(r.entry)).map((r) => r.id);
  if (stale.length === 0) return 0;
  for (let i = 0; i < stale.length; i += 500) {
    const batch = stale.slice(i, i + 500);
    const ph = batch.map(() => '?').join(',');
    await d.execute(`DELETE FROM doc_body WHERE doc_id IN (${ph})`, batch);
    await d.execute(`DELETE FROM docs WHERE id IN (${ph})`, batch);
  }
  return stale.length;
}

/**
 * 把用户输入转成 FTS5 短语查询（改进计划 A6）。
 *
 * MATCH 的参数是**查询语法**而不是普通字符串：只包一层引号时，
 * 输入里出现一个 `"` 就会拼出 `"a"b"` 这种非法语法并抛错。
 * FTS5 的转义规则是把内部双引号写成两个双引号。
 */
export function ftsPhrase(q: string): string {
  return `"${q.replace(/"/g, '""')}"`;
}

/**
 * 跨文件搜索（方案 §7.3）。
 * trigram 分词要求查询词 ≥3 个字符，否则退化为全表扫描——UI 层应提示。
 * snippet 用哨兵串 \u0001/\u0002 包裹命中词，前端转义后再替换为 <mark>（防注入）。
 */
export async function searchDocs(vaultId: number, query: string): Promise<SearchHit[]> {
  const q = query.trim();
  const d = await getDb();
  if (!d || q.length < 3) return [];
  const S = '\u0001';
  const E = '\u0002';
  const rows = await d.select<
    { doc_id: number; entry: string; title: string | null; excerpt: string }[]
  >(
    `SELECT d.id AS doc_id, d.entry, d.title,
            snippet(docs_fts, 1, '${S}', '${E}', '…', 20) AS excerpt
     FROM docs_fts
     JOIN docs d ON d.id = docs_fts.rowid
     WHERE docs_fts MATCH ? AND d.vault_id = ?
     ORDER BY docs_fts.rank
     LIMIT 50`,
    [ftsPhrase(q), vaultId]
  );
  return rows;
}

export async function saveProgress(
  vaultId: number,
  relPath: string,
  anchor: string,
  sectionOffset: number
): Promise<void> {
  // 不建索引也要能存进度（改进计划 A2）
  const docId = await ensureDoc(vaultId, relPath);
  if (docId == null) return;
  const d = await getDb();
  if (!d) return;
  await d.execute(
    'INSERT OR REPLACE INTO progress (doc_id, anchor, section_offset, updated_at) VALUES (?, ?, ?, ?)',
    [docId, anchor, sectionOffset, Date.now()]
  );
  await d.execute('INSERT OR REPLACE INTO recent (doc_id, updated_at) VALUES (?, ?)', [
    docId,
    Date.now(),
  ]);
}

export async function loadProgress(vaultId: number, relPath: string): Promise<ProgressRow | null> {
  const d = await getDb();
  if (!d) return null;
  const rows = await d.select<ProgressRow[]>(
    `SELECT p.anchor, p.section_offset FROM progress p
     JOIN docs d ON d.id = p.doc_id
     WHERE d.vault_id = ? AND d.entry = ?`,
    [vaultId, relPath]
  );
  return rows[0] ?? null;
}

export async function recentDocs(limit = 10): Promise<RecentDoc[]> {
  const d = await getDb();
  if (!d) return [];
  return d.select<RecentDoc[]>(
    `SELECT d.entry, d.title,
            substr(b.body, 1, 120) AS excerpt,
            r.updated_at, v.root_uri AS vault_root
     FROM recent r
     JOIN docs d ON d.id = r.doc_id
     JOIN vaults v ON v.id = d.vault_id
     LEFT JOIN doc_body b ON b.doc_id = d.id
     ORDER BY r.updated_at DESC
     LIMIT ?`,
    [limit]
  );
}

/** 书签（方案 §7.2：文档级收藏 + 段落级书签存 anchor id） */
export interface Bookmark {
  id: number;
  doc_id: number;
  anchor: string;
  note: string | null;
  created_at: number;
  entry: string;
  title: string | null;
  vault_root: string;
}
export async function addBookmark(
  vaultId: number,
  relPath: string,
  anchor: string,
  note?: string
): Promise<void> {
  // 文档可能未被索引，书签仍需要 doc_id 外键（改进计划 A2）
  const docId = await ensureDoc(vaultId, relPath);
  if (docId == null) return;
  const d = await getDb();
  if (!d) return;
  // 去重：同一文档同一 anchor 只保留一条，已存在则更新 note，不再插入。
  // 迁移 v2 已补上 (doc_id, anchor) 唯一索引，这里的查询用于拿到已有 id 以便更新备注。
  const existing = await d.select<{ id: number }[]>(
    'SELECT id FROM bookmarks WHERE doc_id = ? AND anchor = ?',
    [docId, anchor]
  );
  if (existing.length > 0) {
    if (note != null) {
      await d.execute('UPDATE bookmarks SET note = ? WHERE id = ?', [note, existing[0].id]);
    }
    return;
  }
  await d.execute(
    'INSERT INTO bookmarks (doc_id, anchor, note, created_at) VALUES (?, ?, ?, ?)',
    [docId, anchor, note ?? null, Date.now()]
  );
}

export async function removeBookmark(id: number): Promise<void> {
  const d = await getDb();
  await d?.execute('DELETE FROM bookmarks WHERE id = ?', [id]);
}

export async function listBookmarks(vaultId: number): Promise<Bookmark[]> {
  const d = await getDb();
  if (!d) return [];
  return d.select<Bookmark[]>(
    `SELECT b.id, b.doc_id, b.anchor, b.note, b.created_at,
            d.entry, d.title, v.root_uri AS vault_root
     FROM bookmarks b
     JOIN docs d ON d.id = b.doc_id
     JOIN vaults v ON v.id = d.vault_id
     WHERE d.vault_id = ?
     ORDER BY b.created_at DESC`,
    [vaultId]
  );
}
