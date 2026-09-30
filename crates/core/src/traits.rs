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

    // --- Multi-relational memory plane (Jev-Mem inspired) ---

    /// Persist a relation edge. Returns `InvalidParam` for a self-loop/invalid edge
    /// and `NotFound` if either endpoint memory is missing.
    fn add_relation(&self, relation: &Relation) -> Result<()>;

    /// List relations, optionally filtered by `from_id`, `to_id`, and/or `kind`.
    /// `top_k` caps the returned edges (oldest skipped).
    fn get_relations(
        &self,
        from: Option<&MemoId>,
        to: Option<&MemoId>,
        kind: Option<RelationKind>,
        top_k: usize,
    ) -> Result<Vec<Relation>>;

    /// Delete relations matching the given filters. Returns the number removed.
    /// At least one of `from`/`to`/`kind` must be set, otherwise `InvalidParam`.
    fn delete_relations(
        &self,
        from: Option<&MemoId>,
        to: Option<&MemoId>,
        kind: Option<RelationKind>,
    ) -> Result<usize>;

    /// Bounded graph expansion from `query.seeds` across `query.views`. Returns the
    /// reached scored memories plus an inspectable [`RetrieveTrace`].
    fn expand(&self, query: &GraphRetrieveQuery) -> Result<GraphRetrieveResult>;
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

    struct MemStore {
        memos: Mutex<Map<MemoId, Memo>>,
        rels: Mutex<Vec<Relation>>,
    }

    impl MemStore {
        fn new() -> Self {
            MemStore {
                memos: Mutex::new(Map::new()),
                rels: Mutex::new(Vec::new()),
            }
        }
    }

    impl MemoStore for MemStore {
        fn add(&self, m: &Memo) -> Result<()> {
            m.validate()?;
            let mut g = self.memos.lock().unwrap();
            if g.contains_key(&m.id) {
                return Err(MemoError::DuplicateId(m.id.clone()));
            }
            g.insert(m.id.clone(), m.clone());
            Ok(())
        }
        fn get(&self, id: &MemoId) -> Result<Option<Memo>> {
            Ok(self.memos.lock().unwrap().get(id).cloned())
        }
        fn update(&self, m: &Memo) -> Result<()> {
            let mut g = self.memos.lock().unwrap();
            if g.contains_key(&m.id) {
                g.insert(m.id.clone(), m.clone());
                Ok(())
            } else {
                Err(MemoError::NotFound(m.id.clone()))
            }
        }
        fn forget(&self, id: &MemoId) -> Result<bool> {
            Ok(self.memos.lock().unwrap().remove(id).is_some())
        }
        fn search(&self, q: &SearchQuery) -> Result<Vec<ScoredMemo>> {
            q.validate()?;
            let g = self.memos.lock().unwrap();
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
            let g = self.memos.lock().unwrap();
            Ok(g.values()
                .filter(|m| mt.as_ref().is_none_or(|t| t == &m.memo_type))
                .cloned()
                .collect())
        }

        fn add_relation(&self, rel: &Relation) -> Result<()> {
            rel.validate()?;
            let memos = self.memos.lock().unwrap();
            if !memos.contains_key(&rel.from_id) {
                return Err(MemoError::NotFound(rel.from_id.clone()));
            }
            if !memos.contains_key(&rel.to_id) {
                return Err(MemoError::NotFound(rel.to_id.clone()));
            }
            drop(memos);
            let mut rels = self.rels.lock().unwrap();
            if let Some(existing) = rels
                .iter_mut()
                .find(|r| r.from_id == rel.from_id && r.to_id == rel.to_id && r.kind == rel.kind)
            {
                // Idempotent upsert: refresh score/provenance/created_at.
                existing.score = rel.score;
                existing.provenance = rel.provenance.clone();
                existing.created_at = rel.created_at;
            } else {
                rels.push(rel.clone());
            }
            Ok(())
        }

        fn get_relations(
            &self,
            from: Option<&MemoId>,
            to: Option<&MemoId>,
            kind: Option<RelationKind>,
            top_k: usize,
        ) -> Result<Vec<Relation>> {
            let rels = self.rels.lock().unwrap();
            let mut out: Vec<Relation> = rels
                .iter()
                .filter(|r| from.as_ref().is_none_or(|f| **f == r.from_id))
                .filter(|r| to.as_ref().is_none_or(|t| **t == r.to_id))
                .filter(|r| kind.as_ref().is_none_or(|k| k == &r.kind))
                .cloned()
                .collect();
            out.sort_by_key(|r| std::cmp::Reverse(r.created_at));
            out.truncate(top_k);
            Ok(out)
        }

        fn delete_relations(
            &self,
            from: Option<&MemoId>,
            to: Option<&MemoId>,
            kind: Option<RelationKind>,
        ) -> Result<usize> {
            if from.is_none() && to.is_none() && kind.is_none() {
                return Err(MemoError::InvalidParam(
                    "delete_relations needs at least one filter".into(),
                ));
            }
            let mut rels = self.rels.lock().unwrap();
            let before = rels.len();
            rels.retain(|r| {
                let f = from.as_ref().is_none_or(|x| **x == r.from_id);
                let t = to.as_ref().is_none_or(|x| **x == r.to_id);
                let k = kind.as_ref().is_none_or(|x| x == &r.kind);
                // keep when NOT matching all provided filters
                !(f && t && k)
            });
            Ok(before - rels.len())
        }

        fn expand(&self, query: &GraphRetrieveQuery) -> Result<GraphRetrieveResult> {
            query.validate()?;
            let rels = self.rels.lock().unwrap();
            let edges: Map<MemoId, Vec<(MemoId, f32, RelationKind)>> = {
                let mut m = Map::new();
                for r in rels.iter() {
                    m.entry(r.from_id.clone())
                        .or_insert_with(Vec::new)
                        .push((r.to_id.clone(), r.score, r.kind));
                }
                m
            };
            drop(rels);
            let (reached, stop) = graph_bfs(
                &query.seeds,
                &query.views,
                query.budget,
                query.max_hops,
                |n| edges.get(n).cloned().unwrap_or_default(),
            );
            let memos = self.memos.lock().unwrap();
            let mut items = Vec::new();
            for (id, score) in reached.into_iter().take(query.top_k) {
                if let Some(m) = memos.get(&id) {
                    items.push(ScoredMemo {
                        memo: m.clone(),
                        score,
                    });
                }
            }
            let trace = RetrieveTrace {
                views: query.views.clone(),
                budget: query.budget,
                stop_reason: stop,
                hits: items.len(),
            };
            Ok(GraphRetrieveResult { items, trace })
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

    fn put(s: &MemStore, id: &str) {
        let m = Memo {
            id: id.into(),
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
    }

    #[test]
    fn relation_crud_and_expand() {
        let s = MemStore::new();
        put(&s, "a");
        put(&s, "b");
        put(&s, "c");

        // Missing endpoint.
        assert!(matches!(
            s.add_relation(&Relation {
                from_id: "a".into(),
                to_id: "zz".into(),
                kind: RelationKind::Semantic,
                score: 0.8,
                provenance: "local".into(),
                created_at: 1,
            }),
            Err(MemoError::NotFound(_))
        ));

        s.add_relation(&Relation {
            from_id: "a".into(),
            to_id: "b".into(),
            kind: RelationKind::Semantic,
            score: 0.9,
            provenance: "local".into(),
            created_at: 1,
        })
        .unwrap();
        s.add_relation(&Relation {
            from_id: "b".into(),
            to_id: "c".into(),
            kind: RelationKind::Semantic,
            score: 0.7,
            provenance: "local".into(),
            created_at: 2,
        })
        .unwrap();
        // Re-adding the same edge is an idempotent upsert (refreshes score).
        s.add_relation(&Relation {
            from_id: "a".into(),
            to_id: "b".into(),
            kind: RelationKind::Semantic,
            score: 0.99,
            provenance: "local".into(),
            created_at: 3,
        })
        .unwrap();

        let rels = s
            .get_relations(Some(&"a".into()), None, None, 10)
            .unwrap();
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].to_id, "b");
        assert!((rels[0].score - 0.99).abs() < 1e-6);

        // Expand two hops: a -> b -> c.
        let q = GraphRetrieveQuery {
            seeds: vec!["a".into()],
            views: vec![],
            budget: 10,
            max_hops: 3,
            top_k: 10,
        };
        let res = s.expand(&q).unwrap();
        let ids: Vec<&MemoId> = res.items.iter().map(|i| &i.memo.id).collect();
        assert!(ids.contains(&&"a".to_string()));
        assert!(ids.contains(&&"b".to_string()));
        assert!(ids.contains(&&"c".to_string()));
        assert!(!res.trace.stop_reason.is_empty());
        assert_eq!(res.trace.hits, 3);

        // Delete by from.
        let removed = s
            .delete_relations(Some(&"a".into()), None, None)
            .unwrap();
        assert_eq!(removed, 1);
        assert_eq!(s.get_relations(Some(&"a".into()), None, None, 10).unwrap().len(), 0);

        // Empty filter rejected.
        assert!(matches!(
            s.delete_relations(None, None, None),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn expand_rejects_bad_query() {
        let s = MemStore::new();
        put(&s, "a");
        let q = GraphRetrieveQuery::new(vec![]);
        assert!(matches!(s.expand(&q), Err(MemoError::InvalidParam(_))));
    }
}
