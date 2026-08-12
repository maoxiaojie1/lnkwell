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
  rel_path: string;
  title: string | null;
  excerpt: string;
}

export interface ProgressRow {
  anchor: string | null;
  section_offset: number | null;
}

export interface RecentDoc {
  rel_path: string;
  title: string | null;
  excerpt: string | null;
  updated_at: number;
  vault_root: string;
}

const SCHEMA = `
CREATE TABLE IF NOT EXISTS vaults (
  id              INTEGER PRIMARY KEY,
  name            TEXT    NOT NULL,
  root_uri        TEXT    NOT NULL UNIQUE,
  last_opened_at  INTEGER
);
CREATE TABLE IF NOT EXISTS docs (
  id              INTEGER PRIMARY KEY,
  vault_id        INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
  rel_path        TEXT    NOT NULL,
  title           TEXT,
  mtime           INTEGER,
  UNIQUE(vault_id, rel_path)
);
CREATE TABLE IF NOT EXISTS progress (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  anchor          TEXT,
  section_offset  REAL,
  updated_at      INTEGER
);
CREATE TABLE IF NOT EXISTS bookmarks (
  id              INTEGER PRIMARY KEY,
  doc_id          INTEGER NOT NULL REFERENCES docs(id) ON DELETE CASCADE,
  anchor          TEXT    NOT NULL,
  note            TEXT,
  created_at      INTEGER
);
CREATE TABLE IF NOT EXISTS doc_body (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  body            TEXT    NOT NULL
);
CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
  title,
  body,
  content='doc_body',
  content_rowid='doc_id',
  tokenize='trigram'
);
CREATE TRIGGER IF NOT EXISTS doc_body_ai AFTER INSERT ON doc_body BEGIN
  INSERT INTO docs_fts(rowid, title, body)
    VALUES (new.doc_id, (SELECT title FROM docs WHERE id = new.doc_id), new.body);
END;
CREATE TRIGGER IF NOT EXISTS doc_body_ad AFTER DELETE ON doc_body BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, title, body)
    VALUES('delete', old.doc_id, (SELECT title FROM docs WHERE id = old.doc_id), old.body);
END;
CREATE TRIGGER IF NOT EXISTS doc_body_au AFTER UPDATE ON doc_body BEGIN
  INSERT INTO docs_fts(docs_fts, rowid, title, body)
    VALUES('delete', old.doc_id, (SELECT title FROM docs WHERE id = old.doc_id), old.body);
  INSERT INTO docs_fts(rowid, title, body)
    VALUES (new.doc_id, (SELECT title FROM docs WHERE id = new.doc_id), new.body);
END;
CREATE TABLE IF NOT EXISTS render_cache (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  mtime           INTEGER NOT NULL,
  html            TEXT    NOT NULL
);
CREATE TABLE IF NOT EXISTS recent (
  doc_id          INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
  updated_at      INTEGER NOT NULL
);
`;

/**
 * 按语句边界拆分 schema。
 * 不能简单 split(';')：CREATE TRIGGER 的 BEGIN...END 体内含分号，
 * 会被切碎成残缺语句 → SQLite 报 "incomplete input"。
 * 规则：遇到 BEGIN 进入块内，直到匹配的 END 后的 ';' 才算语句结束。
 */
function splitSchema(sql: string): string[] {
  const stmts: string[] = [];
  let cur = '';
  let depth = 0;
  for (const raw of sql.split('\n')) {
    const line = raw.trim();
    cur += raw + '\n';
    // 统计本行的 BEGIN / END（大小写不敏感）
    const begins = (line.match(/\bBEGIN\b/gi) ?? []).length;
    const ends = (line.match(/\bEND\b/gi) ?? []).length;
    depth += begins - ends;
    // 块外（depth<=0）且行以 ; 结尾 → 一条完整语句
    if (depth <= 0 && line.endsWith(';')) {
      const s = cur.trim();
      if (s.length > 1) stmts.push(s.replace(/;+\s*$/, ''));
      cur = '';
      depth = 0;
    }
  }
  const tail = cur.trim();
  if (tail.length > 1) stmts.push(tail.replace(/;+\s*$/, ''));
  return stmts;
}

export async function getDb(): Promise<Database | null> {
  if (db) return db;
  try {
    const d = await Database.load('sqlite:lnkwell.db');
    for (const stmt of splitSchema(SCHEMA)) {
      await d.execute(stmt);
    }
    db = d;
    return d;
  } catch (e) {
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
    'INSERT INTO docs (vault_id, rel_path, title, mtime) VALUES (?, ?, ?, ?) \
     ON CONFLICT(vault_id, rel_path) DO UPDATE SET title = excluded.title, mtime = excluded.mtime',
    [vaultId, relPath, title, mtime]
  );
  const rows = await d.select<{ id: number }[]>(
    'SELECT id FROM docs WHERE vault_id = ? AND rel_path = ?',
    [vaultId, relPath]
  );
  const docId = rows[0]?.id;
  if (docId == null) return;
  await d.execute(
    'INSERT INTO doc_body (doc_id, body) VALUES (?, ?) \
     ON CONFLICT(doc_id) DO UPDATE SET body = excluded.body',
    [docId, plain]
  );
}

/**
 * 跨文件搜索（方案 §7.3）。
 * trigram 分词要求查询词 ≥3 个字符，否则退化为全表扫描——UI 层应提示。
 * snippet 用哨兵串 \u0001/\u0002 包裹命中词，前端转义后再替换为 <mark>（防注入）。
 */
export async function searchDocs(vaultId: number, query: string): Promise<SearchHit[]> {
  const d = await getDb();
  if (!d || query.trim().length < 3) return [];
  const S = '\u0001';
  const E = '\u0002';
  const rows = await d.select<
    { doc_id: number; rel_path: string; title: string | null; excerpt: string }[]
  >(
    `SELECT d.id AS doc_id, d.rel_path, d.title,
            snippet(docs_fts, 1, '${S}', '${E}', '…', 20) AS excerpt
     FROM docs_fts
     JOIN docs d ON d.id = docs_fts.rowid
     WHERE docs_fts MATCH ? AND d.vault_id = ?
     ORDER BY rank
     LIMIT 50`,
    [`"${query.trim()}"`, vaultId]
  );
  return rows;
}

export async function saveProgress(
  vaultId: number,
  relPath: string,
  anchor: string,
  sectionOffset: number
): Promise<void> {
  const d = await getDb();
  if (!d) return;
  const rows = await d.select<{ id: number }[]>(
    'SELECT id FROM docs WHERE vault_id = ? AND rel_path = ?',
    [vaultId, relPath]
  );
  const docId = rows[0]?.id;
  if (docId == null) return;
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
     WHERE d.vault_id = ? AND d.rel_path = ?`,
    [vaultId, relPath]
  );
  return rows[0] ?? null;
}

export async function recentDocs(limit = 10): Promise<RecentDoc[]> {
  const d = await getDb();
  if (!d) return [];
  return d.select<RecentDoc[]>(
    `SELECT d.rel_path, d.title,
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
  rel_path: string;
  title: string | null;
  vault_root: string;
}

export async function addBookmark(
  vaultId: number,
  relPath: string,
  anchor: string,
  note?: string
): Promise<void> {
  const d = await getDb();
  if (!d) return;
  // 确保 docs 表有记录（文档可能未被索引，书签需要 doc_id 外键）
  await d.execute(
    'INSERT INTO docs (vault_id, rel_path, title, mtime) VALUES (?, ?, ?, ?) \
     ON CONFLICT(vault_id, rel_path) DO NOTHING',
    [vaultId, relPath, null, 0]
  );
  const rows = await d.select<{ id: number }[]>(
    'SELECT id FROM docs WHERE vault_id = ? AND rel_path = ?',
    [vaultId, relPath]
  );
  const docId = rows[0]?.id;
  if (docId == null) return;
  // 去重：同一文档同一 anchor 只保留一条，已存在则更新 note，不再插入
  // （bookmarks 表无 UNIQUE 约束，必须代码层判断；老库可能已有重复数据，
  //  加 CREATE UNIQUE INDEX 会失败并使整个 getDb 返回 null，故不依赖约束）
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
            d.rel_path, d.title, v.root_uri AS vault_root
     FROM bookmarks b
     JOIN docs d ON d.id = b.doc_id
     JOIN vaults v ON v.id = d.vault_id
     WHERE d.vault_id = ?
     ORDER BY b.created_at DESC`,
    [vaultId]
  );
}
