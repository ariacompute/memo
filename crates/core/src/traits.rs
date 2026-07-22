use crate::error::Result;
use crate::model::*;

/// 记忆存储抽象：增删改查与检索。
pub trait MemoryStore: Send + Sync {
    /// 新增一条记忆；重复 id 返回 `DuplicateId`。
    fn add(&self, memory: &Memory) -> Result<()>;

    /// 按 id 获取；不存在返回 `None`。
    fn get(&self, id: &MemoryId) -> Result<Option<Memory>>;

    /// 按 id 更新（需已存在），不存在返回 `NotFound`。
    fn update(&self, memory: &Memory) -> Result<()>;

    /// 按 id 遗忘；返回是否实际删除。
    fn forget(&self, id: &MemoryId) -> Result<bool>;

    /// 混合检索（语义 + 关键词）。
    fn search(&self, query: &SearchQuery) -> Result<Vec<ScoredMemory>>;

    /// 列出记忆（可按类型过滤）；供巩固/去重/CLI 使用。
    fn list(&self, memory_type: Option<MemoryType>) -> Result<Vec<Memory>>;

    /// 批量新增（默认逐条；后端可覆盖为事务实现）。
    fn add_batch(&self, memories: &[Memory]) -> Result<()> {
        for m in memories {
            self.add(m)?;
        }
        Ok(())
    }
}

/// 文本嵌入抽象。可注入本地或第三方 embedder。
pub trait Embedder: Send + Sync {
    /// 将文本编码为定长向量；空文本/零向量返回 `EmptyEmbedding`。
    fn embed(&self, text: &str) -> Result<Vec<f32>>;
    /// 向量维度。
    fn dim(&self) -> usize;
}

/// 底层持久化后端抽象（M1 仅 SQLite 实现；复制后端为后续里程碑，rqlite 灵感）。
pub trait StorageBackend: Send + Sync {
    /// 执行建表/迁移。
    fn migrate(&self) -> Result<()>;
    /// 后端种类标识。
    fn backend_kind(&self) -> &'static str;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::MemoryError;

    // 内存版 MemoryStore，用于在不依赖 SQLite 的情况下验证 trait 契约。
    use std::collections::HashMap as Map;
    use std::sync::Mutex;

    struct MemStore(Mutex<Map<MemoryId, Memory>>);

    impl MemStore {
        fn new() -> Self {
            MemStore(Mutex::new(Map::new()))
        }
    }

    impl MemoryStore for MemStore {
        fn add(&self, m: &Memory) -> Result<()> {
            m.validate()?;
            let mut g = self.0.lock().unwrap();
            if g.contains_key(&m.id) {
                return Err(MemoryError::DuplicateId(m.id.clone()));
            }
            g.insert(m.id.clone(), m.clone());
            Ok(())
        }
        fn get(&self, id: &MemoryId) -> Result<Option<Memory>> {
            Ok(self.0.lock().unwrap().get(id).cloned())
        }
        fn update(&self, m: &Memory) -> Result<()> {
            let mut g = self.0.lock().unwrap();
            if g.contains_key(&m.id) {
                g.insert(m.id.clone(), m.clone());
                Ok(())
            } else {
                Err(MemoryError::NotFound(m.id.clone()))
            }
        }
        fn forget(&self, id: &MemoryId) -> Result<bool> {
            Ok(self.0.lock().unwrap().remove(id).is_some())
        }
        fn search(&self, q: &SearchQuery) -> Result<Vec<ScoredMemory>> {
            q.validate()?;
            let g = self.0.lock().unwrap();
            let mut out: Vec<ScoredMemory> = g
                .values()
                .filter(|m| q.memory_type.as_ref().is_none_or(|t| t == &m.memory_type))
                .map(|m| {
                    let s = keyword_score(&m.content, &q.text);
                    ScoredMemory {
                        memory: m.clone(),
                        score: q.keyword_weight * s,
                    }
                })
                .filter(|r| r.score >= q.score_threshold)
                .collect();
            out.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap());
            out.truncate(q.top_k);
            Ok(out)
        }
        fn list(&self, mt: Option<MemoryType>) -> Result<Vec<Memory>> {
            let g = self.0.lock().unwrap();
            Ok(g.values()
                .filter(|m| mt.as_ref().is_none_or(|t| t == &m.memory_type))
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
        let mut m = crate::model::Memory {
            id: "a".into(),
            memory_type: MemoryType::Working,
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
        // 重复 id
        s.add(&m).unwrap();
        let dup = Memory {
            id: "a".into(),
            ..m.clone()
        };
        assert!(matches!(s.add(&dup), Err(MemoryError::DuplicateId(_))));
        // 空内容
        let mut bad = m.clone();
        bad.content = "".into();
        assert!(matches!(s.add(&bad), Err(MemoryError::EmptyContent)));
        let _ = &mut m;
    }

    #[test]
    fn embedder_dim_and_value() {
        let e = ConstEmbedder;
        assert_eq!(e.dim(), 3);
        assert_eq!(e.embed("anything").unwrap(), vec![1.0, 0.0, 0.0]);
    }
}
