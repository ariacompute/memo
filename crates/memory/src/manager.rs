use memory_core::*;
use memory_embed::cosine;
use memory_storage::SqliteStore;
use std::collections::HashMap;
use std::sync::Arc;

/// 记忆管理器：组合嵌入器与存储后端，提供高层记忆操作。
pub struct MemoryManager {
    embedder: Arc<dyn Embedder>,
    store: Arc<dyn MemoryStore>,
}

impl MemoryManager {
    pub fn new(embedder: Arc<dyn Embedder>, store: Arc<dyn MemoryStore>) -> Self {
        Self { embedder, store }
    }

    /// 便捷构造：使用 SQLite 后端。
    pub fn with_sqlite(embedder: Arc<dyn Embedder>, db_path: &str) -> Result<Self> {
        let store = SqliteStore::open(db_path)?;
        Ok(Self::new(embedder, Arc::new(store)))
    }

    /// 新增记忆：自动嵌入并持久化。返回生成的 id。
    pub fn add(
        &self,
        content: &str,
        memory_type: MemoryType,
        metadata: HashMap<String, String>,
        importance: f32,
    ) -> Result<MemoryId> {
        let content = content.to_string();
        if content.trim().is_empty() {
            return Err(MemoryError::EmptyContent);
        }
        if !(0.0..=1.0).contains(&importance) {
            return Err(MemoryError::InvalidParam("importance out of [0,1]".into()));
        }
        let emb = self.embedder.embed(&content)?;
        let now = now_secs();
        let m = Memory {
            id: generate_id(),
            memory_type,
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

    pub fn get(&self, id: &MemoryId) -> Result<Option<Memory>> {
        self.store.get(id)
    }

    /// 更新记忆；内容变更时重算嵌入、版本 +1。
    pub fn update(&self, id: &MemoryId, patch: MemoryPatch) -> Result<()> {
        if patch.is_empty() {
            return Err(MemoryError::InvalidParam("empty patch".into()));
        }
        let mut m = self
            .store
            .get(id)?
            .ok_or_else(|| MemoryError::NotFound(id.clone()))?;
        if let Some(c) = patch.content {
            if c.trim().is_empty() {
                return Err(MemoryError::EmptyContent);
            }
            m.content = c;
            m.embedding = Some(self.embedder.embed(&m.content)?);
        }
        if let Some(t) = patch.memory_type {
            m.memory_type = t;
        }
        if let Some(meta) = patch.metadata {
            m.metadata = meta;
        }
        if let Some(imp) = patch.importance {
            if !(0.0..=1.0).contains(&imp) {
                return Err(MemoryError::InvalidParam("importance out of [0,1]".into()));
            }
            m.importance = imp;
        }
        m.version += 1;
        m.updated_at = now_secs();
        self.store.update(&m)
    }

    pub fn forget(&self, id: &MemoryId) -> Result<bool> {
        self.store.forget(id)
    }

    /// 混合检索：自动嵌入查询文本后委托存储层打分。
    pub fn search(&self, mut query: SearchQuery) -> Result<Vec<ScoredMemory>> {
        query.validate()?;
        if query.query_embedding.is_none() {
            let emb = self.embedder.embed(&query.text)?;
            query.query_embedding = Some(emb);
        }
        self.store.search(&query)
    }

    /// 巩固：提升（或降低）记忆重要性，夹紧到 [0,1]。
    pub fn consolidate(&self, id: &MemoryId, delta: f32) -> Result<()> {
        let mut m = self
            .store
            .get(id)?
            .ok_or_else(|| MemoryError::NotFound(id.clone()))?;
        m.importance = (m.importance + delta).clamp(0.0, 1.0);
        m.version += 1;
        m.updated_at = now_secs();
        self.store.update(&m)
    }

    /// 去重：相似度 >= 阈值 的记忆合并入保留项（拼接内容、刷新时间戳）。
    pub fn dedup(&self, threshold: f32) -> Result<usize> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(MemoryError::InvalidParam("threshold out of [0,1]".into()));
        }
        let mut all = self.store.list(None)?;
        all.sort_by(|a, b| {
            b.importance
                .partial_cmp(&a.importance)
                .unwrap()
                .then(b.updated_at.cmp(&a.updated_at))
        });
        let mut kept: Vec<Memory> = Vec::new();
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

    pub fn list(&self, memory_type: Option<MemoryType>) -> Result<Vec<Memory>> {
        self.store.list(memory_type)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_embed::LocalEmbedder;
    use std::sync::Arc;

    fn mgr() -> MemoryManager {
        let e: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
        let s = SqliteStore::open(":memory:").unwrap();
        MemoryManager::new(e, Arc::new(s))
    }

    #[test]
    fn golden_path_add_search_get() {
        let m = mgr();
        let id = m
            .add("user prefers rust for systems programming", MemoryType::Working, HashMap::new(), 0.8)
            .unwrap();
        let mut q = SearchQuery::new("rust systems");
        q.top_k = 5;
        let rs = m.search(q).unwrap();
        assert!(!rs.is_empty());
        assert_eq!(rs[0].memory.id, id);
        let got = m.get(&id).unwrap().unwrap();
        assert!(got.content.contains("rust"));
        assert!(got.embedding.is_some());
    }

    #[test]
    fn add_validation_errors() {
        let m = mgr();
        assert!(matches!(
            m.add("", MemoryType::Working, HashMap::new(), 0.5),
            Err(MemoryError::EmptyContent)
        ));
        assert!(matches!(
            m.add("x", MemoryType::Working, HashMap::new(), 1.5),
            Err(MemoryError::InvalidParam(_))
        ));
    }

    #[test]
    fn update_changes_content_and_embedding() {
        let m = mgr();
        let id = m.add("old content here", MemoryType::ShortTerm, HashMap::new(), 0.5).unwrap();
        m.update(
            &id,
            MemoryPatch {
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
                MemoryPatch {
                    content: Some("x".into()),
                    ..Default::default()
                }
            ),
            Err(MemoryError::NotFound(_))
        ));
        // 空补丁返回 InvalidParam，而非 NotFound
        assert!(matches!(
            m.update(&"missing".to_string(), MemoryPatch::default()),
            Err(MemoryError::InvalidParam(_))
        ));
    }

    #[test]
    fn forget_returns_bool() {
        let m = mgr();
        let id = m.add("to forget", MemoryType::Working, HashMap::new(), 0.5).unwrap();
        assert!(m.forget(&id).unwrap());
        assert!(!m.forget(&id).unwrap());
    }

    #[test]
    fn consolidate_clamps() {
        let m = mgr();
        let id = m.add("imp", MemoryType::Working, HashMap::new(), 0.5).unwrap();
        m.consolidate(&id, 1.0).unwrap();
        assert!((m.get(&id).unwrap().unwrap().importance - 1.0).abs() < 1e-6);
        m.consolidate(&id, -5.0).unwrap();
        assert!((m.get(&id).unwrap().unwrap().importance).abs() < 1e-6);
    }

    #[test]
    fn dedup_merges_similar() {
        let m = mgr();
        let _a = m.add("user likes rust language", MemoryType::LongTerm { kind: LongTermKind::Semantic }, HashMap::new(), 0.9).unwrap();
        let _b = m.add("user likes rust language", MemoryType::LongTerm { kind: LongTermKind::Semantic }, HashMap::new(), 0.4).unwrap();
        let removed = m.dedup(0.95).unwrap();
        assert_eq!(removed, 1);
        assert_eq!(m.list(None).unwrap().len(), 1);
    }
}
