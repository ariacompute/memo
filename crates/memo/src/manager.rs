use memo_core::*;
use memo_embed::cosine;
use memo_storage::SqliteStore;
use std::collections::HashMap;
use std::sync::Arc;

/// Memory manager: combines an embedder with a storage backend to expose high-level memory operations.
pub struct MemoManager {
    embedder: Arc<dyn Embedder>,
    store: Arc<dyn MemoStore>,
}

impl MemoManager {
    pub fn new(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoStore>) -> Self {
        Self { embedder, store }
    }

    /// Convenience constructor using the SQLite backend.
    pub fn with_sqlite(embedder: Arc<dyn Embedder>, db_path: &str) -> Result<Self> {
        let store = SqliteStore::open(db_path)?;
        Ok(Self::new(embedder, Arc::new(store)))
    }

    /// Add a memory: automatically embeds and persists it, returning the generated id.
    pub fn add(
        &self,
        content: &str,
        memo_type: MemoType,
        metadata: HashMap<String, String>,
        importance: f32,
    ) -> Result<MemoId> {
        let content = content.to_string();
        if content.trim().is_empty() {
            return Err(MemoError::EmptyContent);
        }
        if !(0.0..=1.0).contains(&importance) {
            return Err(MemoError::InvalidParam("importance out of [0,1]".into()));
        }
        let emb = self.embedder.embed(&content)?;
        let now = now_secs();
        let m = Memo {
            id: generate_id(),
            memo_type,
            content,
            embedding: Some(emb),
            metadata,
            importance,
            version: 1,
            created_at: now,
            updated_at: now,
        };
        self.store.add(&m)?;
        Ok(m.id)
    }

    pub fn get(&self, id: &MemoId) -> Result<Option<Memo>> {
        self.store.get(id)
    }

    /// Pure vector (semantic) recall: embed the query text and rank stored
    /// memories by cosine similarity only — no keyword component. This is the
    /// dedicated "向量召回" entrypoint, distinct from the hybrid [`search`].
    pub fn recall(&self, mut query: RecallQuery) -> Result<Vec<ScoredMemo>> {
        query.validate()?;
        if query.query_embedding.is_none() {
            let emb = self.embedder.embed(&query.text)?;
            query.query_embedding = Some(emb);
        }
        // Delegate to the storage layer as a semantic-only search
        // (semantic_weight=1, keyword_weight=0).
        let mut sq = SearchQuery::new(query.text.clone());
        sq.top_k = query.top_k;
        sq.score_threshold = query.score_threshold;
        sq.memo_type = query.memo_type.clone();
        sq.semantic_weight = 1.0;
        sq.keyword_weight = 0.0;
        sq.query_embedding = query.query_embedding;
        self.store.search(&sq)
    }

    /// Update a memory; recomputes the embedding and bumps the version when content changes.
    pub fn update(&self, id: &MemoId, patch: MemoPatch) -> Result<()> {
        if patch.is_empty() {
            return Err(MemoError::InvalidParam("empty patch".into()));
        }
        let mut m = self
            .store
            .get(id)?
            .ok_or_else(|| MemoError::NotFound(id.clone()))?;
        if let Some(c) = patch.content {
            if c.trim().is_empty() {
                return Err(MemoError::EmptyContent);
            }
            m.content = c;
            m.embedding = Some(self.embedder.embed(&m.content)?);
        }
        if let Some(t) = patch.memo_type {
            m.memo_type = t;
        }
        if let Some(meta) = patch.metadata {
            m.metadata = meta;
        }
        if let Some(imp) = patch.importance {
            if !(0.0..=1.0).contains(&imp) {
                return Err(MemoError::InvalidParam("importance out of [0,1]".into()));
            }
            m.importance = imp;
        }
        m.version += 1;
        m.updated_at = now_secs();
        self.store.update(&m)
    }

    pub fn forget(&self, id: &MemoId) -> Result<bool> {
        self.store.forget(id)
    }

    /// Hybrid search: embeds the query text then delegates scoring to the storage layer.
    pub fn search(&self, mut query: SearchQuery) -> Result<Vec<ScoredMemo>> {
        query.validate()?;
        if query.query_embedding.is_none() {
            let emb = self.embedder.embed(&query.text)?;
            query.query_embedding = Some(emb);
        }
        self.store.search(&query)
    }

    /// Batch hybrid search: embeds each query (if needed) and scores all of them
    /// under a single storage lock via `search_batch`.
    pub fn search_batch(&self, queries: &[SearchQuery]) -> Result<Vec<Vec<ScoredMemo>>> {
        let mut qs: Vec<SearchQuery> = Vec::with_capacity(queries.len());
        for q in queries {
            let mut q = q.clone();
            q.validate()?;
            if q.query_embedding.is_none() {
                let emb = self.embedder.embed(&q.text)?;
                q.query_embedding = Some(emb);
            }
            qs.push(q);
        }
        self.store.search_batch(&qs)
    }

    /// Batch pure-vector recall (semantic only, `keyword_weight = 0`).
    pub fn recall_batch(&self, queries: &[RecallQuery]) -> Result<Vec<Vec<ScoredMemo>>> {
        let mut qs: Vec<SearchQuery> = Vec::with_capacity(queries.len());
        for q in queries {
            let mut sq = SearchQuery::new(q.text.clone());
            sq.top_k = q.top_k;
            sq.score_threshold = q.score_threshold;
            sq.memo_type = q.memo_type.clone();
            sq.semantic_weight = 1.0;
            sq.keyword_weight = 0.0;
            sq.query_embedding = q.query_embedding.clone();
            if sq.query_embedding.is_none() {
                let emb = self.embedder.embed(&q.text)?;
                sq.query_embedding = Some(emb);
            }
            qs.push(sq);
        }
        self.store.search_batch(&qs)
    }

    /// Consolidate: raise (or lower) a memory's importance, clamped to [0,1].
    pub fn consolidate(&self, id: &MemoId, delta: f32) -> Result<()> {
        let mut m = self
            .store
            .get(id)?
            .ok_or_else(|| MemoError::NotFound(id.clone()))?;
        m.importance = (m.importance + delta).clamp(0.0, 1.0);
        m.version += 1;
        m.updated_at = now_secs();
        self.store.update(&m)
    }

    /// Deduplicate: memories with similarity >= threshold are merged into the kept item
    /// (content is concatenated and the timestamp is refreshed).
    pub fn dedup(&self, threshold: f32) -> Result<usize> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(MemoError::InvalidParam("threshold out of [0,1]".into()));
        }
        let mut all = self.store.list(None)?;
        all.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap()
                .then(b.updated_at.cmp(&a.updated_at))
        });
        let mut kept: Vec<Memo> = Vec::new();
        let mut removed = 0;
        for m in all {
            let mut merged = false;
            for k in kept.iter_mut() {
                if let (Some(a), Some(b)) = (&k.embedding, &m.embedding) {
                    if let Ok(s) = cosine(a, b) {
                        if s >= threshold {
                            k.content = format!("{} | {}", k.content, m.content);
                            k.updated_at = now_secs();
                            k.version += 1;
                            self.store.update(k)?;
                            self.store.forget(&m.id)?;
                            removed += 1;
                            merged = true;
                            break;
                        }
                    }
                }
            }
            if !merged {
                kept.push(m);
            }
        }
        Ok(removed)
    }

    pub fn list(&self, memo_type: Option<MemoType>) -> Result<Vec<Memo>> {
        self.store.list(memo_type)
    }

    /// Access the underlying embedder (used by the relation controller).
    pub fn embedder(&self) -> Arc<dyn Embedder> {
        self.embedder.clone()
    }

    /// Access the underlying store (used by the relation controller and CLI).
    pub fn store(&self) -> Arc<dyn MemoStore> {
        self.store.clone()
    }
}

// ---------------------------------------------------------------------------
// Multi-relational controller (Jev-Mem System-One analog, local heuristics)
// ---------------------------------------------------------------------------

/// Tunables for the relation controller.
#[derive(Debug, Clone)]
pub struct RelationConfig {
    /// How many existing memories to evaluate as relation candidates per write.
    pub candidate_top_k: usize,
    /// Minimum score to accept an inferred relation edge.
    pub relation_threshold: f32,
    /// Cap on edges created in a single write→connect pass.
    pub max_relations_per_write: usize,
}

impl Default for RelationConfig {
    fn default() -> Self {
        Self {
            candidate_top_k: 10,
            relation_threshold: 0.6,
            max_relations_per_write: 8,
        }
    }
}

/// Pluggable relation scorer. The default [`LocalRelationScorer`] runs fully
/// offline; swap in an LLM-backed implementation without touching the controller.
pub trait RelationScorer: Send + Sync {
    fn score(&self, a: &Memo, b: &Memo, kind: RelationKind) -> f32;
}

/// Lightweight tokenizer for entity/keyword overlap.
fn tokens(s: &str) -> std::collections::HashSet<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| w.len() >= 2)
        .map(|w| w.to_string())
        .collect()
}

fn token_jaccard(a: &str, b: &str) -> f32 {
    let ta = tokens(a);
    let tb = tokens(b);
    if ta.is_empty() || tb.is_empty() {
        return 0.0;
    }
    let inter = ta.intersection(&tb).count() as f32;
    let union = ta.union(&tb).count() as f32;
    inter / union
}

/// Default offline heuristic scorer mirroring Jev-Mem's four relation views.
pub struct LocalRelationScorer;

impl RelationScorer for LocalRelationScorer {
    fn score(&self, a: &Memo, b: &Memo, kind: RelationKind) -> f32 {
        match kind {
            RelationKind::Semantic => match (&a.embedding, &b.embedding) {
                (Some(x), Some(y)) => cosine(x, y).unwrap_or(0.0),
                _ => keyword_score(&a.content, &b.content),
            },
            RelationKind::Temporal => {
                // Deterministic: an earlier memory can link forward to a later one.
                if a.created_at <= b.created_at {
                    1.0
                } else {
                    0.0
                }
            }
            RelationKind::Entity => token_jaccard(&a.content, &b.content),
            RelationKind::Causal => {
                // Heuristic: chronologically adjacent + keyword/entity overlap.
                let adj = if a.created_at <= b.created_at { 0.5 } else { 0.0 };
                let overlap = token_jaccard(&a.content, &b.content);
                (adj + 0.5 * overlap).min(1.0)
            }
        }
    }
}

/// Local control plane: write→connect relation inference and
/// retrieve→assess→expand bounded graph traversal.
pub struct MemoryController {
    embedder: Arc<dyn Embedder>,
    store: Arc<dyn MemoStore>,
    scorer: Arc<dyn RelationScorer>,
    cfg: RelationConfig,
}

impl MemoryController {
    pub fn new(
        embedder: Arc<dyn Embedder>,
        store: Arc<dyn MemoStore>,
        scorer: Arc<dyn RelationScorer>,
        cfg: RelationConfig,
    ) -> Self {
        Self {
            embedder,
            store,
            scorer,
            cfg,
        }
    }

    /// Convenience constructor with the offline default scorer and config.
    pub fn with_defaults(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoStore>) -> Self {
        Self::new(embedder, store, Arc::new(LocalRelationScorer), RelationConfig::default())
    }

    /// Write→connect: after `memo` is persisted, pick bounded candidates and
    /// infer four-view relations, persisting edges above the threshold.
    /// Returns the number of edges created.
    pub fn connect(&self, memo: &Memo) -> Result<usize> {
        // The source memory must already be persisted (write→connect ordering).
        if self.store.get(&memo.id)?.is_none() {
            return Err(MemoError::NotFound(memo.id.clone()));
        }
        let candidates = self.candidate_memos(memo)?;
        let mut added = 0;
        for cand in candidates.iter().take(self.cfg.max_relations_per_write) {
            for kind in [
                RelationKind::Semantic,
                RelationKind::Temporal,
                RelationKind::Causal,
                RelationKind::Entity,
            ] {
                let sc = self.scorer.score(memo, cand, kind);
                if sc < self.cfg.relation_threshold {
                    continue;
                }
                let now = now_secs();
                // Symmetric views (semantic/entity/causal) get both directions;
                // temporal stays chronological (memo -> earlier candidate only).
                let forward = kind == RelationKind::Temporal && memo.created_at > cand.created_at;
                if !forward {
                    self.store.add_relation(&Relation {
                        from_id: memo.id.clone(),
                        to_id: cand.id.clone(),
                        kind,
                        score: sc,
                        provenance: "local".into(),
                        created_at: now,
                    })?;
                    added += 1;
                }
                if kind != RelationKind::Temporal {
                    self.store.add_relation(&Relation {
                        from_id: cand.id.clone(),
                        to_id: memo.id.clone(),
                        kind,
                        score: sc,
                        provenance: "local".into(),
                        created_at: now,
                    })?;
                    added += 1;
                }
            }
        }
        Ok(added)
    }

    /// Retrieve→assess→expand: hybrid search seeds anchors, then bounded graph
    /// expansion across `views` returns scored memories plus an inspectable trace.
    pub fn retrieve(
        &self,
        text: &str,
        views: Vec<RelationKind>,
        budget: usize,
        max_hops: usize,
        top_k: usize,
    ) -> Result<GraphRetrieveResult> {
        if budget == 0 || max_hops == 0 || top_k == 0 {
            return Err(MemoError::InvalidParam(
                "retrieve budget/max_hops/top_k must be > 0".into(),
            ));
        }
        let mut sq = SearchQuery::new(text);
        sq.top_k = self.cfg.candidate_top_k;
        sq.semantic_weight = 0.7;
        sq.keyword_weight = 0.5;
        if sq.query_embedding.is_none() {
            sq.query_embedding = Some(self.embedder.embed(text)?);
        }
        let seeds: Vec<MemoId> = self
            .store
            .search(&sq)?
            .into_iter()
            .map(|s| s.memo.id)
            .collect();
        if seeds.is_empty() {
            return Ok(GraphRetrieveResult {
                items: Vec::new(),
                trace: RetrieveTrace {
                    views,
                    budget,
                    stop_reason: "no-seeds".into(),
                    hits: 0,
                },
            });
        }
        let q = GraphRetrieveQuery {
            seeds,
            views,
            budget,
            max_hops,
            top_k,
        };
        self.store.expand(&q)
    }

    /// Nearest existing memories to `memo` (excluding itself), bounded by config.
    fn candidate_memos(&self, memo: &Memo) -> Result<Vec<Memo>> {
        let mut sq = SearchQuery::new(memo.content.clone());
        sq.top_k = self.cfg.candidate_top_k + 1;
        sq.semantic_weight = 0.7;
        sq.keyword_weight = 0.5;
        sq.query_embedding = memo.embedding.clone();
        let mut cands = self.store.search(&sq)?;
        cands.retain(|s| s.memo.id != memo.id);
        Ok(cands.into_iter().map(|s| s.memo).collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memo_embed::LocalEmbedder;
    use std::sync::Arc;

    fn mgr() -> MemoManager {
        let e: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
        let s = SqliteStore::open(":memory:").unwrap();
        MemoManager::new(e, Arc::new(s))
    }

    #[test]
    fn golden_path_add_search_get() {
        let m = mgr();
        let id = m
            .add("user prefers rust for systems programming", MemoType::Working, HashMap::new(), 0.8)
            .unwrap();
        let mut q = SearchQuery::new("rust systems");
        q.top_k = 5;
        let rs = m.search(q).unwrap();
        assert!(!rs.is_empty());
        assert_eq!(rs[0].memo.id, id);
        let got = m.get(&id).unwrap().unwrap();
        assert!(got.content.contains("rust"));
        assert!(got.embedding.is_some());
    }

    #[test]
    fn vector_recall_ranks_similar() {
        let m = mgr();
        let rust = m
            .add("user prefers rust for systems programming", MemoType::Working, HashMap::new(), 0.8)
            .unwrap();
        m.add("banana smoothie recipe with ice", MemoType::Working, HashMap::new(), 0.5)
            .unwrap();
        let rs = m.recall(RecallQuery::new("rust programming language")).unwrap();
        assert!(!rs.is_empty());
        // The semantically similar rust memory must rank first.
        assert_eq!(rs[0].memo.id, rust);
        // Score is pure cosine similarity in [0, 1].
        assert!((0.0..=1.0).contains(&rs[0].score));
    }

    #[test]
    fn add_validation_errors() {
        let m = mgr();
        assert!(matches!(
            m.add("", MemoType::Working, HashMap::new(), 0.5),
            Err(MemoError::EmptyContent)
        ));
        assert!(matches!(
            m.add("x", MemoType::Working, HashMap::new(), 1.5),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn update_changes_content_and_embedding() {
        let m = mgr();
        let id = m.add("old content here", MemoType::ShortTerm, HashMap::new(), 0.5).unwrap();
        m.update(
            &id,
            MemoPatch {
                content: Some("new fresh content".into()),
                ..Default::default()
            },
        )
        .unwrap();
        let got = m.get(&id).unwrap().unwrap();
        assert_eq!(got.content, "new fresh content");
        assert_eq!(got.version, 2);
    }

    #[test]
    fn update_missing_is_not_found() {
        let m = mgr();
        assert!(matches!(
            m.update(
                &"missing".to_string(),
                MemoPatch {
                    content: Some("x".into()),
                    ..Default::default()
                }
            ),
            Err(MemoError::NotFound(_))
        ));
        // An empty patch returns InvalidParam, not NotFound
        assert!(matches!(
            m.update(&"missing".to_string(), MemoPatch::default()),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn forget_returns_bool() {
        let m = mgr();
        let id = m.add("to forget", MemoType::Working, HashMap::new(), 0.5).unwrap();
        assert!(m.forget(&id).unwrap());
        assert!(!m.forget(&id).unwrap());
    }

    #[test]
    fn consolidate_clamps() {
        let m = mgr();
        let id = m.add("imp", MemoType::Working, HashMap::new(), 0.5).unwrap();
        m.consolidate(&id, 1.0).unwrap();
        assert!((m.get(&id).unwrap().unwrap().importance - 1.0).abs() < 1e-6);
        m.consolidate(&id, -5.0).unwrap();
        assert!((m.get(&id).unwrap().unwrap().importance).abs() < 1e-6);
    }

    #[test]
    fn dedup_merges_similar() {
        let m = mgr();
        let _a = m.add("user likes rust language", MemoType::LongTerm { kind: LongTermKind::Semantic }, HashMap::new(), 0.9).unwrap();
        let _b = m.add("user likes rust language", MemoType::LongTerm { kind: LongTermKind::Semantic }, HashMap::new(), 0.4).unwrap();
        let removed = m.dedup(0.95).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(m.list(None).unwrap().len(), 1);
    }

    #[test]
    fn update_type_only_bumps_version() {
        let m = mgr();
        let id = m.add("plain content", MemoType::Working, HashMap::new(), 0.5).unwrap();
        m.update(
            &id,
            MemoPatch {
                memo_type: Some(MemoType::ShortTerm),
                ..Default::default()
            },
        )
        .unwrap();
        let got = m.get(&id).unwrap().unwrap();
        assert_eq!(got.memo_type, MemoType::ShortTerm);
        assert_eq!(got.version, 2);
        assert_eq!(got.content, "plain content");
    }

    #[test]
    fn update_importance_only() {
        let m = mgr();
        let id = m.add("c", MemoType::Working, HashMap::new(), 0.5).unwrap();
        m.update(
            &id,
            MemoPatch {
                importance: Some(0.2),
                ..Default::default()
            },
        )
        .unwrap();
        assert!((m.get(&id).unwrap().unwrap().importance - 0.2).abs() < 1e-6);
    }

    #[test]
    fn update_empty_content_and_importance_range_errors() {
        let m = mgr();
        let id = m.add("c", MemoType::Working, HashMap::new(), 0.5).unwrap();
        assert!(matches!(
            m.update(
                &id,
                MemoPatch {
                    content: Some("   ".into()),
                    ..Default::default()
                }
            ),
            Err(MemoError::EmptyContent)
        ));
        assert!(matches!(
            m.update(
                &id,
                MemoPatch {
                    importance: Some(1.5),
                    ..Default::default()
                }
            ),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn search_and_recall_reject_invalid_query() {
        let m = mgr();
        m.add("x", MemoType::Working, HashMap::new(), 0.5).unwrap();
        // empty text -> validation error before embedding
        assert!(matches!(m.search(SearchQuery::new("")), Err(MemoError::InvalidParam(_))));
        assert!(matches!(m.recall(RecallQuery::new("")), Err(MemoError::InvalidParam(_))));
    }

    #[test]
    fn recall_with_provided_embedding_skips_embedder() {
        let m = mgr();
        let id = m.add("user prefers rust", MemoType::Working, HashMap::new(), 0.8).unwrap();
        let mut q = RecallQuery::new("rust");
        q.query_embedding = Some(vec![1.0, 0.0, 0.0]); // injected; embedder must NOT be called
        let rs = m.recall(q).unwrap();
        assert!(rs.iter().any(|r| r.memo.id == id));
    }

    #[test]
    fn dedup_invalid_threshold_and_noop() {
        let m = mgr();
        assert!(matches!(m.dedup(2.0), Err(MemoError::InvalidParam(_))));
        m.add("unique one", MemoType::Working, HashMap::new(), 0.5).unwrap();
        m.add("unique two", MemoType::Working, HashMap::new(), 0.5).unwrap();
        // dissimilar -> nothing merged
        assert_eq!(m.dedup(0.99).unwrap(), 0);
        assert_eq!(m.list(None).unwrap().len(), 2);
    }

    #[test]
    fn consolidate_missing_id_is_not_found() {
        let m = mgr();
        assert!(matches!(
            m.consolidate(&"missing".to_string(), 0.1),
            Err(MemoError::NotFound(_))
        ));
    }

    #[test]
    fn with_sqlite_builds_in_memory() {
        let e: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
        let m = MemoManager::with_sqlite(e, ":memory:").unwrap();
        let id = m.add("probe", MemoType::Working, HashMap::new(), 0.5).unwrap();
        assert!(m.get(&id).unwrap().is_some());
    }

    fn manual_memo(manager: &MemoManager, id: &str, content: &str, ts: i64) -> Memo {
        let emb = manager.embedder.embed(content).unwrap();
        Memo {
            id: id.into(),
            memo_type: MemoType::Working,
            content: content.into(),
            embedding: Some(emb),
            metadata: HashMap::new(),
            importance: 0.5,
            version: 1,
            created_at: ts,
            updated_at: ts,
        }
    }

    #[test]
    fn controller_write_connect_and_retrieve() {
        let mgr = mgr();
        // Two similar memories written at different times.
        let a = manual_memo(&mgr, "a", "user likes rust programming language", 100);
        mgr.store.add(&a).unwrap();
        let b = manual_memo(&mgr, "b", "user likes rust systems programming", 200);
        mgr.store.add(&b).unwrap();

        let ctrl = MemoryController::with_defaults(
            mgr.embedder.clone(),
            mgr.store.clone(),
        );
        // Connect b -> graph should infer semantic/entity edges to a.
        let n = ctrl.connect(&b).unwrap();
        assert!(n >= 2, "expected symmetric semantic+entity edges, got {n}");

        // Retrieving from text near b should reach a via the graph.
        let res = ctrl
            .retrieve("rust programming", vec![], 20, 3, 10)
            .unwrap();
        let ids: Vec<&MemoId> = res.items.iter().map(|i| &i.memo.id).collect();
        assert!(ids.contains(&&"a".to_string()));
        assert!(ids.contains(&&"b".to_string()));
        assert!(!res.trace.stop_reason.is_empty());
        assert_eq!(res.trace.hits, ids.len());

        // Invalid retrieve params rejected.
        assert!(ctrl.retrieve("x", vec![], 0, 3, 10).is_err());
    }

    #[test]
    fn controller_connect_rejects_missing_endpoint() {
        let mgr = mgr();
        let a = manual_memo(&mgr, "a", "standalone memory", 1);
        let ctrl = MemoryController::with_defaults(mgr.embedder.clone(), mgr.store.clone());
        // a is not persisted; connect must fail with NotFound.
        assert!(matches!(ctrl.connect(&a), Err(MemoError::NotFound(_))));
    }

    #[test]
    fn local_relation_scorer_stays_in_range() {
        let m = mgr();
        let a = manual_memo(&m, "a", "user likes rust programming language", 100);
        let b = manual_memo(&m, "b", "rust systems programming by the user", 200);
        let scorer = LocalRelationScorer;
        for k in [
            RelationKind::Semantic,
            RelationKind::Temporal,
            RelationKind::Causal,
            RelationKind::Entity,
        ] {
            let s = scorer.score(&a, &b, k);
            assert!(
                (0.0..=1.0).contains(&s),
                "LocalRelationScorer {k:?} out of [0,1]: {s}"
            );
        }
    }

    #[test]
    fn connect_builds_symmetric_and_temporal_directed() {
        let mgr = mgr();
        // a earlier, b later
        let a = manual_memo(&mgr, "a", "user likes rust", 100);
        let b = manual_memo(&mgr, "b", "user likes rust systems", 200);
        mgr.store.add(&a).unwrap();
        mgr.store.add(&b).unwrap();
        let ctrl = MemoryController::with_defaults(mgr.embedder.clone(), mgr.store.clone());
        // Connect the EARLIER memory so temporal edges (earlier -> later) are produced.
        let n = ctrl.connect(&a).unwrap();
        assert!(n >= 2, "expected symmetric + temporal edges, got {n}");

        let from_a = ctrl.store.get_relations(Some(&"a".into()), None, None, 50).unwrap();
        let to_a = ctrl.store.get_relations(None, Some(&"a".into()), None, 50).unwrap();
        let temporal_from_a = from_a.iter().any(|r| r.kind == RelationKind::Temporal);
        let temporal_to_a = to_a.iter().any(|r| r.kind == RelationKind::Temporal);
        // temporal only points from earlier -> later (a -> b), never reversed.
        assert!(temporal_from_a, "expected a->b temporal edge");
        assert!(!temporal_to_a, "temporal must not be reversed (b->a)");
        // semantic/entity/causal are symmetric: a must also have incoming edges.
        assert!(!to_a.is_empty(), "symmetric views must create edges into a");
    }

    #[test]
    fn retrieve_returns_seeds_and_trace() {
        let mgr = mgr();
        let a = manual_memo(&mgr, "a", "user likes rust programming", 100);
        let b = manual_memo(&mgr, "b", "rust systems programming by user", 200);
        mgr.store.add(&a).unwrap();
        mgr.store.add(&b).unwrap();
        let ctrl = MemoryController::with_defaults(mgr.embedder.clone(), mgr.store.clone());
        ctrl.connect(&a).unwrap();
        // Retrieval from a query near the memories returns scored items + a populated trace.
        let res = ctrl.retrieve("rust programming", vec![], 20, 3, 10).unwrap();
        let ids: Vec<&MemoId> = res.items.iter().map(|i| &i.memo.id).collect();
        assert!(ids.contains(&&"a".to_string()));
        assert!(ids.contains(&&"b".to_string()));
        assert!(!res.trace.stop_reason.is_empty());
        assert_eq!(res.trace.hits, ids.len());
        // Invalid retrieve params rejected.
        assert!(ctrl.retrieve("x", vec![], 0, 3, 10).is_err());
        assert!(ctrl.retrieve("x", vec![], 10, 0, 10).is_err());
        assert!(ctrl.retrieve("x", vec![], 10, 3, 0).is_err());
    }
}
