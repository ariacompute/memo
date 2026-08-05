use memo_core::*;
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;
use std::sync::Mutex;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS memories (
    id TEXT PRIMARY KEY,
    memo_type TEXT NOT NULL,
    content TEXT NOT NULL,
    embedding BLOB,
    metadata TEXT NOT NULL,
    importance REAL NOT NULL,
    version INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    deleted INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX IF NOT EXISTS idx_memories_type ON memories(memo_type);
CREATE INDEX IF NOT EXISTS idx_memories_updated_at ON memories(updated_at);
CREATE INDEX IF NOT EXISTS idx_memories_deleted ON memories(deleted);
";

/// 嵌入式 SQLite 记忆存储后端。
pub struct SqliteStore {
    conn: Mutex<Connection>,
}

fn db_err(e: rusqlite::Error) -> MemoError {
    // 还原 row 映射中内裹的 MemoError（如损坏 BLOB / 元数据解析失败）。
    if let rusqlite::Error::FromSqlConversionFailure(_, _, inner) = &e {
        if let Some(me) = inner.downcast_ref::<MemoError>() {
            return me.clone();
        }
    }
    MemoError::Db(e.to_string())
}

fn serialize_embedding(emb: Option<&[f32]>) -> Result<Vec<u8>> {
    match emb {
        None => Ok(Vec::new()),
        Some(v) => {
            let mut buf = Vec::with_capacity(4 + v.len() * 4);
            buf.extend_from_slice(&(v.len() as u32).to_le_bytes());
            for f in v {
                buf.extend_from_slice(&f.to_le_bytes());
            }
            Ok(buf)
        }
    }
}

fn deserialize_embedding(buf: &[u8]) -> Result<Option<Vec<f32>>> {
    if buf.is_empty() {
        return Ok(None);
    }
    if buf.len() < 4 {
        return Err(MemoError::Serialization("embedding blob too short".into()));
    }
    let n = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]) as usize;
    if buf.len() != 4 + n * 4 {
        return Err(MemoError::Serialization("embedding blob length mismatch".into()));
    }
    let mut v = Vec::with_capacity(n);
    for i in 0..n {
        let s = 4 + i * 4;
        let f = f32::from_le_bytes([buf[s], buf[s + 1], buf[s + 2], buf[s + 3]]);
        v.push(f);
    }
    Ok(Some(v))
}

fn row_to_memo(row: &Row) -> rusqlite::Result<Memo> {
    let id: String = row.get(0)?;
    let mt: String = row.get(1)?;
    let content: String = row.get(2)?;
    let emb_blob: Vec<u8> = row.get(3)?;
    let meta: String = row.get(4)?;
    let importance: f64 = row.get(5)?;
    let version: i64 = row.get(6)?;
    let created: i64 = row.get(7)?;
    let updated: i64 = row.get(8)?;
    let embedding = deserialize_embedding(&emb_blob).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(3, rusqlite::types::Type::Blob, Box::new(e))
    })?;
    let metadata: std::collections::HashMap<String, String> = serde_json::from_str(&meta).map_err(
        |e| {
            rusqlite::Error::FromSqlConversionFailure(
                4,
                rusqlite::types::Type::Text,
                Box::new(MemoError::Serialization(e.to_string())),
            )
        },
    )?;
    let memo_type = <MemoType as std::str::FromStr>::from_str(&mt).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(
            1,
            rusqlite::types::Type::Text,
            Box::new(e),
        )
    })?;
    Ok(Memo {
        id,
        memo_type,
        content,
        embedding,
        metadata,
        importance: importance as f32,
        version: version as u64,
        created_at: created,
        updated_at: updated,
    })
}

/// 内联余弦相似度（避免 storage 依赖 embed 层）；维度不一致/空返回 None。
fn cosine(a: &[f32], b: &[f32]) -> Option<f32> {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return None;
    }
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return Some(0.0);
    }
    Some(dot / (na * nb))
}

impl SqliteStore {
    /// 打开（或创建）数据库并迁移。`:memory:` 表示内存库；文件路径会自动创建父目录。
    pub fn open(location: &str) -> Result<Self> {
        let conn = if location == ":memory:" {
            Connection::open_in_memory().map_err(db_err)?
        } else {
            if let Some(parent) = Path::new(location).parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent).map_err(MemoError::Io)?;
                }
            }
            Connection::open(location).map_err(db_err)?
        };
        let store = Self {
            conn: Mutex::new(conn),
        };
        store.migrate()?;
        Ok(store)
    }

    /// 建表与索引。
    pub fn migrate(&self) -> Result<()> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        conn.execute_batch(SCHEMA).map_err(db_err)?;
        Ok(())
    }

    fn insert(&self, conn: &Connection, m: &Memo) -> Result<()> {
        let emb = serialize_embedding(m.embedding.as_deref())?;
        let meta =
            serde_json::to_string(&m.metadata).map_err(|e| MemoError::Serialization(e.to_string()))?;
        conn.execute(
            "INSERT INTO memories (id, memo_type, content, embedding, metadata, importance, version, created_at, updated_at, deleted) \
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 0)",
            params![
                m.id,
                m.memo_type.as_str(),
                m.content,
                emb,
                meta,
                m.importance,
                m.version as i64,
                m.created_at,
                m.updated_at
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }
}

impl StorageBackend for SqliteStore {
    fn migrate(&self) -> Result<()> {
        self.migrate()
    }
    fn backend_kind(&self) -> &'static str {
        "sqlite"
    }
}

impl MemoStore for SqliteStore {
    fn add(&self, m: &Memo) -> Result<()> {
        m.validate()?;
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM memories WHERE id=?1",
                [m.id.clone()],
                |_| Ok(true),
            )
            .optional()
            .map_err(db_err)?
            .is_some();
        if exists {
            return Err(MemoError::DuplicateId(m.id.clone()));
        }
        self.insert(&conn, m)
    }

    fn get(&self, id: &MemoId) -> Result<Option<Memo>> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let m = conn
            .query_row(
                "SELECT id, memo_type, content, embedding, metadata, importance, version, created_at, updated_at \
                 FROM memories WHERE id=?1 AND deleted=0",
                [id.clone()],
                row_to_memo,
            )
            .optional()
            .map_err(db_err)?;
        Ok(m)
    }

    fn update(&self, m: &Memo) -> Result<()> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM memories WHERE id=?1 AND deleted=0",
                [m.id.clone()],
                |_| Ok(true),
            )
            .optional()
            .map_err(db_err)?
            .is_some();
        if !exists {
            return Err(MemoError::NotFound(m.id.clone()));
        }
        let emb = serialize_embedding(m.embedding.as_deref())?;
        let meta =
            serde_json::to_string(&m.metadata).map_err(|e| MemoError::Serialization(e.to_string()))?;
        conn.execute(
            "UPDATE memories SET memo_type=?2, content=?3, embedding=?4, metadata=?5, importance=?6, version=?7, updated_at=?8 WHERE id=?1",
            params![
                m.id,
                m.memo_type.as_str(),
                m.content,
                emb,
                meta,
                m.importance,
                m.version as i64,
                m.updated_at
            ],
        )
        .map_err(db_err)?;
        Ok(())
    }

    fn forget(&self, id: &MemoId) -> Result<bool> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let n = conn
            .execute(
                "DELETE FROM memories WHERE id=?1 AND deleted=0",
                [id.clone()],
            )
            .map_err(db_err)?;
        Ok(n > 0)
    }

    fn list(&self, memo_type: Option<MemoType>) -> Result<Vec<Memo>> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let mut sql = String::from(
            "SELECT id, memo_type, content, embedding, metadata, importance, version, created_at, updated_at \
             FROM memories WHERE deleted=0",
        );
        let mut strs: Vec<String> = Vec::new();
        if let Some(t) = &memo_type {
            sql.push_str(" AND memo_type=?");
            strs.push(t.as_str());
        }
        sql.push_str(" ORDER BY updated_at DESC");
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(strs.iter()), row_to_memo)
            .map_err(db_err)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r.map_err(db_err)?);
        }
        Ok(out)
    }

    fn search(&self, q: &SearchQuery) -> Result<Vec<ScoredMemo>> {
        q.validate()?;
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let mut sql = String::from(
            "SELECT id, memo_type, content, embedding, metadata, importance, version, created_at, updated_at \
             FROM memories WHERE deleted=0",
        );
        let mut strs: Vec<String> = Vec::new();
        if let Some(t) = &q.memo_type {
            sql.push_str(" AND memo_type=?");
            strs.push(t.as_str());
        }
        sql.push_str(" ORDER BY updated_at DESC");
        let mut stmt = conn.prepare(&sql).map_err(db_err)?;
        let rows = stmt
            .query_map(rusqlite::params_from_iter(strs.iter()), row_to_memo)
            .map_err(db_err)?;
        let mut candidates: Vec<Memo> = Vec::new();
        for r in rows {
            candidates.push(r.map_err(db_err)?);
        }
        drop(stmt);
        drop(conn);

        let query_emb = q.query_embedding.as_deref();
        let mut scored: Vec<ScoredMemo> = candidates
            .into_iter()
            .map(|m| {
                let semantic = match (query_emb, m.embedding.as_deref()) {
                    (Some(a), Some(b)) => cosine(a, b).unwrap_or(0.0),
                    _ => 0.0,
                };
                let keyword = keyword_score(&m.content, &q.text);
                let score = q.semantic_weight * semantic + q.keyword_weight * keyword;
                ScoredMemo { memo: m, score }
            })
            .filter(|r| r.score >= q.score_threshold)
            .collect();
        scored.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
        scored.truncate(q.top_k);
        Ok(scored)
    }

    fn add_batch(&self, memories: &[Memo]) -> Result<()> {
        let conn = self.conn.lock().expect("sqlite lock poisoned");
        let tx = conn.unchecked_transaction().map_err(db_err)?;
        for m in memories {
            m.validate()?;
            let exists: bool = tx
                .query_row(
                    "SELECT 1 FROM memories WHERE id=?1",
                    [m.id.clone()],
                    |_| Ok(true),
                )
                .optional()
                .map_err(db_err)?
                .is_some();
            if exists {
                return Err(MemoError::DuplicateId(m.id.clone()));
            }
            self.insert(&tx, m)?;
        }
        tx.commit().map_err(db_err)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    fn mem(id: &str, content: &str, embedding: Option<Vec<f32>>) -> Memo {
        Memo {
            id: id.into(),
            memo_type: MemoType::LongTerm {
                kind: LongTermKind::Episodic,
            },
            content: content.into(),
            embedding,
            metadata: HashMap::new(),
            importance: 0.5,
            version: 1,
            created_at: 1,
            updated_at: 1,
        }
    }

    #[test]
    fn add_get_update_forget() {
        let s = SqliteStore::open(":memory:").unwrap();
        let m = mem("a", "hello world", Some(vec![0.1, 0.2, 0.3]));
        s.add(&m).unwrap();
        assert!(s.get(&"a".into()).unwrap().is_some());
        // 重复 id
        assert!(matches!(s.add(&m), Err(MemoError::DuplicateId(_))));
        let mut m2 = m.clone();
        m2.content = "hello rust".into();
        m2.embedding = Some(vec![0.3, 0.2, 0.1]);
        m2.version = 2;
        m2.updated_at = 2;
        s.update(&m2).unwrap();
        let got = s.get(&"a".into()).unwrap().unwrap();
        assert_eq!(got.content, "hello rust");
        // 更新不存在
        let mut m3 = m.clone();
        m3.id = "nope".into();
        assert!(matches!(s.update(&m3), Err(MemoError::NotFound(_))));
        // forget
        assert!(s.forget(&"a".into()).unwrap());
        assert!(!s.forget(&"a".into()).unwrap());
        assert!(s.get(&"a".into()).unwrap().is_none());
    }

    #[test]
    fn empty_content_rejected() {
        let s = SqliteStore::open(":memory:").unwrap();
        let mut m = mem("a", "x", None);
        m.content = "   ".into();
        assert!(matches!(s.add(&m), Err(MemoError::EmptyContent)));
    }

    #[test]
    fn list_and_search() {
        let s = SqliteStore::open(":memory:").unwrap();
        s.add(&mem("a", "user likes rust programming", Some(vec![0.9, 0.1, 0.0]))).unwrap();
        s.add(&mem("b", "banana smoothie recipe", Some(vec![0.0, 0.9, 0.1]))).unwrap();
        assert_eq!(s.list(None).unwrap().len(), 2);
        // keyword 检索
        let q = SearchQuery::new("rust");
        let r = s.search(&q).unwrap();
        assert_eq!(r[0].memo.id, "a");
        // 类型过滤
        let mut q2 = SearchQuery::new("rust");
        q2.memo_type = Some(MemoType::Working);
        assert!(s.search(&q2).unwrap().is_empty());
    }

    #[test]
    fn batch_insert_atomic_duplicate() {
        let s = SqliteStore::open(":memory:").unwrap();
        let mut dup = mem("a", "x", None);
        dup.id = "a".into();
        let mut dup2 = mem("a", "y", None);
        dup2.id = "a".into();
        assert!(matches!(
            s.add_batch(&[dup, dup2]),
            Err(MemoError::DuplicateId(_))
        ));
        // 因事务回滚，a 不应被写入
        assert!(s.get(&"a".into()).unwrap().is_none());
    }

    #[test]
    fn corrupted_db_fails_to_open() {
        let dir = std::env::temp_dir();
        let path = dir.join(format!("mem_corrupt_{}.db", std::process::id()));
        std::fs::write(&path, b"this is not a sqlite database").unwrap();
        let res = SqliteStore::open(path.to_str().unwrap());
        assert!(res.is_err());
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn corrupt_embedding_blob_errors_on_read() {
        let s = SqliteStore::open(":memory:").unwrap();
        s.conn
            .lock()
            .unwrap()
            .execute(
                "INSERT INTO memories (id, memo_type, content, embedding, metadata, importance, version, created_at, updated_at, deleted) \
                 VALUES ('x','working','c',X'00','{}',0.5,1,0,0,0)",
                [],
            )
            .unwrap();
        let r = s.get(&"x".into());
        assert!(matches!(r, Err(MemoError::Serialization(_))));
    }
}
