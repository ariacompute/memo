use crate::error::Result;
use crate::model::*;

/// Memory storage abstraction: CRUD and retrieval.
pub trait MemoStore: Send + Sync {
    /// Add a memory; returns `DuplicateId` for a repeated id.
    fn add(&self, memo: &Memo) -> Result<()>;

    /// Fetch by id; returns `None` if not present.
    fn get(&self, id: &MemoId) -> Result<Option<Memo>>;

    /// Update by id (must already exist); returns `NotFound` if absent.
    fn update(&self, memo: &Memo) -> Result<()>;

    /// Forget by id; returns whether a memory was actually removed.
    fn forget(&self, id: &MemoId) -> Result<bool>;

    /// Hybrid retrieval (semantic + keyword).
    fn search(&self, query: &SearchQuery) -> Result<Vec<ScoredMemo>>;

    /// List memories (optionally filtered by type); used by consolidation/dedup/CLI.
    fn list(&self, memo_type: Option<MemoType>) -> Result<Vec<Memo>>;

    /// Batch add (default: one by one; backends may override with a transactional implementation).
    fn add_batch(&self, memories: &[Memo]) -> Result<()> {
        for m in memories {
            self.add(m)?;
        }
        Ok(())
    }
}

/// Text embedding abstraction. Allows injecting a local or third-party embedder.
pub trait Embedder: Send + Sync {
    /// Encode text into a fixed-length vector; returns `EmptyEmbedding` for empty text/zero vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    /// Vector dimension.
    fn dim(&self) -> usize;
}

/// Low-level persistence backend abstraction (M1 only implements SQLite; a replicated
/// backend is a later milestone, inspired by rqlite).
pub trait StorageBackend: Send + Sync {
    /// Run schema creation/migration.
    fn migrate(&self) -> Result<()>;
    /// Backend kind identifier.
    fn backend_kind(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MemoError;

    // In-memory MemoStore used to verify the trait contract without depending on SQLite.
    use std::collections::HashMap as Map;
    use std::sync::Mutex;

    struct MemStore(Mutex<Map<MemoId, Memo>>);

    impl MemStore {
        fn new() -> Self {
            MemStore(Mutex::new(Map::new()))
        }
    }

    impl MemoStore for MemStore {
        fn add(&self, m: &Memo) -> Result<()> {
            m.validate()?;
            let mut g = self.0.lock().unwrap();
            if g.contains_key(&m.id) {
                return Err(MemoError::DuplicateId(m.id.clone()));
            }
            g.insert(m.id.clone(), m.clone());
            Ok(())
        }
        fn get(&self, id: &MemoId) -> Result<Option<Memo>> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn update(&self, m: &Memo) -> Result<()> {
            let mut g = self.0.lock().unwrap();
            if g.contains_key(&m.id) {
                g.insert(m.id.clone(), m.clone());
                Ok(())
            } else {
                Err(MemoError::NotFound(m.id.clone()))
            }
        }
        fn forget(&self, id: &MemoId) -> Result<bool> {
            Ok(self.0.lock().unwrap().remove(id).is_some())
        }
        fn search(&self, q: &SearchQuery) -> Result<Vec<ScoredMemo>> {
            q.validate()?;
            let g = self.0.lock().unwrap();
            let mut out: Vec<ScoredMemo> = g
                .values()
                .filter(|m| q.memo_type.as_ref().is_none_or(|t| t == &m.memo_type))
                .map(|m| {
                    let s = keyword_score(&m.content, &q.text);
                    ScoredMemo {
                        memo: m.clone(),
                        score: q.keyword_weight * s,
                    }
                })
                .filter(|r| r.score >= q.score_threshold)
                .collect();
            out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            out.truncate(q.top_k);
            Ok(out)
        }
        fn list(&self, mt: Option<MemoType>) -> Result<Vec<Memo>> {
            let g = self.0.lock().unwrap();
            Ok(g.values()
                .filter(|m| mt.as_ref().is_none_or(|t| t == &m.memo_type))
                .cloned()
                .collect())
        }
    }

    struct ConstEmbedder;

    impl Embedder for ConstEmbedder {
        fn embed(&self, _text: &str) -> Result<Vec<f32>> {
            Ok(vec![1.0, 0.0, 0.0])
        }
        fn dim(&self) -> usize {
            3
        }
    }

    #[test]
    fn trait_contract_add_get_forget() {
        let s = MemStore::new();
        let mut m = crate::model::Memo {
            id: "a".into(),
            memo_type: MemoType::Working,
            content: "x".into(),
            embedding: None,
            metadata: Map::new(),
            importance: 0.5,
            version: 1,
            created_at: 0,
            updated_at: 0,
        };
        s.add(&m).unwrap();
        assert!(s.get(&"a".into()).unwrap().is_some());
        assert!(s.forget(&"a".into()).unwrap());
        assert!(!s.forget(&"a".into()).unwrap());
        // Duplicate id
        s.add(&m).unwrap();
        let dup = Memo {
            id: "a".into(),
            ..m.clone()
        };
        assert!(matches!(s.add(&dup), Err(MemoError::DuplicateId(_))));
        // Empty content
        let mut bad = m.clone();
        bad.content = "".into();
        assert!(matches!(s.add(&bad), Err(MemoError::EmptyContent)));
        let _ = &mut m;
    }

    #[test]
    fn embedder_dim_and_value() {
        let e = ConstEmbedder;
        assert_eq!(e.dim(), 3);
        assert_eq!(e.embed("anything").unwrap(), vec![1.0, 0.0, 0.0]);
    }
}
