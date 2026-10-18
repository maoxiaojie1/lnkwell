"""FTS5 检索层自检（改进计划 A6 / A7 的实测闭环）。

不依赖 Tauri 与前端测试框架：用 Python 自带的 sqlite3 建出与 `src/lib/db.ts`
**完全一致**的 schema，然后验证检索与对账的每一条 SQL 行为。

它覆盖了三件在 TypeScript 侧无法断言的事：
  1. FTS5 外部内容表的列必须与 content 表一致（否则 snippet() 抛 SQL logic error）；
  2. MATCH 的参数是查询语法，未转义的用户输入会破坏它；
  3. 老库（doc_body 无 title 列）能被迁移 v4 修复。

运行：
    python scripts/verify-fts.py
退出码 0 表示全部通过。
"""

import sqlite3
import sys

FAILURES = []


def check(label, cond, detail=""):
    if cond:
        print(f"  [OK]   {label}")
    else:
        print(f"  [FAIL] {label} {detail}")
        FAILURES.append(label)


# ---------------------------------------------------------------- schema

FTS_SCHEMA = [
    """CREATE VIRTUAL TABLE IF NOT EXISTS docs_fts USING fts5(
        title, body, content='doc_body', content_rowid='doc_id', tokenize='trigram')""",
    """CREATE TRIGGER IF NOT EXISTS doc_body_ai AFTER INSERT ON doc_body BEGIN
        INSERT INTO docs_fts(rowid, title, body) VALUES (new.doc_id, new.title, new.body);
    END""",
    """CREATE TRIGGER IF NOT EXISTS doc_body_ad AFTER DELETE ON doc_body BEGIN
        INSERT INTO docs_fts(docs_fts, rowid, title, body)
          VALUES('delete', old.doc_id, old.title, old.body);
    END""",
    """CREATE TRIGGER IF NOT EXISTS doc_body_au AFTER UPDATE ON doc_body BEGIN
        INSERT INTO docs_fts(docs_fts, rowid, title, body)
          VALUES('delete', old.doc_id, old.title, old.body);
        INSERT INTO docs_fts(rowid, title, body) VALUES (new.doc_id, new.title, new.body);
    END""",
]

SCHEMA_V1 = [
    """CREATE TABLE IF NOT EXISTS vaults (
        id INTEGER PRIMARY KEY, name TEXT NOT NULL,
        root_uri TEXT NOT NULL UNIQUE, last_opened_at INTEGER)""",
    """CREATE TABLE IF NOT EXISTS docs (
        id INTEGER PRIMARY KEY,
        vault_id INTEGER NOT NULL REFERENCES vaults(id) ON DELETE CASCADE,
        entry TEXT NOT NULL, title TEXT, mtime INTEGER,
        UNIQUE(vault_id, entry))""",
    """CREATE TABLE IF NOT EXISTS progress (
        doc_id INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
        anchor TEXT, section_offset REAL, updated_at INTEGER)""",
    """CREATE TABLE IF NOT EXISTS bookmarks (
        id INTEGER PRIMARY KEY,
        doc_id INTEGER NOT NULL REFERENCES docs(id) ON DELETE CASCADE,
        anchor TEXT NOT NULL, note TEXT, created_at INTEGER)""",
    """CREATE TABLE IF NOT EXISTS doc_body (
        doc_id INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
        title TEXT, body TEXT NOT NULL)""",
    *FTS_SCHEMA,
    """CREATE TABLE IF NOT EXISTS recent (
        doc_id INTEGER PRIMARY KEY REFERENCES docs(id) ON DELETE CASCADE,
        updated_at INTEGER NOT NULL)""",
]

SEARCH_SQL = """SELECT d.id AS doc_id, d.entry, d.title,
        snippet(docs_fts, 1, '\x01', '\x02', '…', 20) AS excerpt
 FROM docs_fts JOIN docs d ON d.id = docs_fts.rowid
 WHERE docs_fts MATCH ? AND d.vault_id = ?
 ORDER BY docs_fts.rank LIMIT 50"""


def fts_phrase(q):
    """与 db.ts / ftsPhrase 完全一致"""
    return '"' + q.replace('"', '""') + '"'


def new_db():
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    for stmt in SCHEMA_V1:
        db.execute(stmt)
    db.execute("INSERT INTO vaults (name, root_uri) VALUES ('v', '/tmp/v')")
    return db


def add_doc(db, entry, title, body):
    db.execute("INSERT INTO docs (vault_id, entry, title) VALUES (1, ?, ?)", (entry, title))
    did = db.execute("SELECT id FROM docs WHERE entry = ?", (entry,)).fetchone()[0]
    db.execute(
        "INSERT INTO doc_body (doc_id, title, body) VALUES (?, ?, ?)", (did, title, body)
    )
    return did


def search(db, q):
    return db.execute(SEARCH_SQL, (fts_phrase(q), 1)).fetchall()


# ---------------------------------------------------------------- 用例

def case_readable(db):
    print("\n[1] 索引可读（旧实现在这里就炸了）")
    try:
        n = db.execute("SELECT count(*) FROM docs_fts").fetchone()[0]
        check("SELECT count(*) FROM docs_fts", n == 3, f"rows={n}")
    except sqlite3.Error as e:
        check("SELECT count(*) FROM docs_fts", False, f"{type(e).__name__}: {e}")


def case_search(db):
    print("\n[2] 中文子串检索 + snippet + JOIN + rank")
    hits = search(db, "环境配置")
    check("trigram 中文子串命中", len(hits) == 1 and hits[0][1] == "a.md", str(hits))
    check("snippet 带哨兵包裹", hits and "\x01" in hits[0][3], str(hits))
    check("JOIN 后 title 可读", hits and hits[0][2] == "配置指南", str(hits))

    print("\n[3] 标题也参与检索")
    by_title = search(db, "扩散模型讲义")
    check("命中标题", any(r[1] == "b.md" for r in by_title), str(by_title))


def case_escaping(db):
    print("\n[4] 用户输入转义（A6）")
    try:
        db.execute(SEARCH_SQL, ('"a"b"', 1)).fetchall()
        check("未转义输入应报错（证明旧实现在此崩溃）", False)
    except sqlite3.Error as e:
        check("未转义输入报错", True, f"{type(e).__name__}")

    for q in ['a"b', "(", "*", '"', "环境配置", ""]:
        try:
            search(db, q)
            check(f"转义后可用 q={q!r}", True)
        except sqlite3.Error as e:
            check(f"转义后可用 q={q!r}", False, f"{type(e).__name__}: {e}")


def case_sync(db):
    print("\n[5] 触发器单向同步（INSERT / UPDATE / DELETE）")
    did = add_doc(db, "d.md", "临时标题", "独一无二的短语甲")
    check("插入后可搜到", any(r[1] == "d.md" for r in search(db, "独一无二的短语甲")))
    db.execute(
        "UPDATE doc_body SET title = '改后标题', body = '重新写的短语乙' WHERE doc_id = ?", (did,)
    )
    check("更新后新词可搜到", any(r[1] == "d.md" for r in search(db, "重新写的短语乙")))
    check("更新后旧词已失效", not any(r[1] == "d.md" for r in search(db, "独一无二的短语甲")))
    db.execute("DELETE FROM doc_body WHERE doc_id = ?", (did,))
    db.execute("DELETE FROM docs WHERE id = ?", (did,))
    check("删除后不再命中", not any(r[1] == "d.md" for r in search(db, "重新写的短语乙")))


def case_prune(db):
    print("\n[6] 索引对账（A7）：删除顺序与残留")
    before = db.execute("SELECT count(*) FROM docs_fts").fetchone()[0]
    target = db.execute("SELECT id FROM docs WHERE entry = 'a.md'").fetchone()[0]
    # 当前实现：先删 doc_body（触发器读到 old.title/old.body），再删 docs
    db.execute("DELETE FROM doc_body WHERE doc_id = ?", (target,))
    db.execute("DELETE FROM docs WHERE id = ?", (target,))
    after = db.execute("SELECT count(*) FROM docs_fts").fetchone()[0]
    docs_left = db.execute("SELECT count(*) FROM docs").fetchone()[0]
    check("FTS 行数与 docs 行数一致", after == docs_left, f"fts={after} docs={docs_left}")
    check("确实少了一条", after == before - 1, f"{before} -> {after}")
    check("已删文档不再命中", not any(r[1] == "a.md" for r in search(db, "环境配置")))


def case_legacy_migration():
    print("\n[7] 老库迁移（doc_body 无 title → 迁移 v4 修复）")
    db = sqlite3.connect(":memory:")
    db.execute("PRAGMA foreign_keys = ON")
    # 旧结构：doc_body 只有 body；fts 声明了 title；触发器跨表子查询
    db.execute("CREATE TABLE docs (id INTEGER PRIMARY KEY, vault_id INTEGER, entry TEXT, title TEXT)")
    db.execute("CREATE TABLE doc_body (doc_id INTEGER PRIMARY KEY, body TEXT NOT NULL)")
    db.execute(
        "CREATE VIRTUAL TABLE docs_fts USING fts5("
        "title, body, content='doc_body', content_rowid='doc_id', tokenize='trigram')"
    )
    db.execute(
        """CREATE TRIGGER doc_body_ai AFTER INSERT ON doc_body BEGIN
           INSERT INTO docs_fts(rowid, title, body)
             VALUES (new.doc_id, (SELECT title FROM docs WHERE id = new.doc_id), new.body);
        END"""
    )
    db.execute("INSERT INTO docs (id, vault_id, entry, title) VALUES (1, 1, 'a.md', '配置指南')")
    db.execute("INSERT INTO doc_body (doc_id, body) VALUES (1, '环境配置说明')")
    db.execute("INSERT INTO docs_fts(rowid, title, body) VALUES (1, '配置指南', '环境配置说明')")

    def legacy_search_works():
        try:
            db.execute(SEARCH_SQL, (fts_phrase("环境配置"), 1)).fetchall()
            return True
        except sqlite3.Error:
            return False

    check("旧结构下 snippet 报错（复现线上 bug）", not legacy_search_works())

    # ---- 迁移 v4 的四个步骤 ----
    db.execute("DROP TRIGGER IF EXISTS doc_body_ai")
    db.execute("DROP TRIGGER IF EXISTS doc_body_ad")
    db.execute("DROP TRIGGER IF EXISTS doc_body_au")
    db.execute("DROP TABLE IF EXISTS docs_fts")
    db.execute("ALTER TABLE doc_body ADD COLUMN title TEXT")
    db.execute("UPDATE doc_body SET title = (SELECT title FROM docs WHERE id = doc_body.doc_id)")
    for stmt in FTS_SCHEMA:
        db.execute(stmt)
    db.execute("INSERT INTO docs_fts(docs_fts) VALUES('rebuild')")

    try:
        rows = db.execute(SEARCH_SQL, (fts_phrase("环境配置"), 1)).fetchall()
    except sqlite3.Error as e:
        check("迁移后 snippet 可用", False, f"{type(e).__name__}: {e}")
        return
    check("迁移后 snippet 可用", len(rows) == 1, str(rows))
    check("迁移后 title 已回填", rows and rows[0][2] == "配置指南", str(rows))


def main():
    print(f"SQLite {sqlite3.sqlite_version}（FTS5 trigram 需要 >= 3.34）")
    db = new_db()
    add_doc(db, "a.md", "配置指南", "环境配置说明：如何设置代理")
    add_doc(db, "b.md", "扩散模型讲义", "扩散模型的推导过程")
    add_doc(db, "c.md", "特殊字符", '引言里有一个"引号"和 * 星号 ( 括号')

    case_readable(db)
    case_search(db)
    case_escaping(db)
    case_sync(db)
    case_prune(db)
    case_legacy_migration()

    print("\n" + ("全部通过" if not FAILURES else f"失败 {len(FAILURES)} 项: {FAILURES}"))
    return 0 if not FAILURES else 1


if __name__ == "__main__":
    sys.exit(main())
