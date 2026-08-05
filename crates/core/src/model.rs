use crate::error::{MemoError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 记忆唯一标识。
pub type MemoId = String;

/// 长期记忆子类型（参考 mem0）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LongTermKind {
    Episodic,
    Semantic,
    Entity,
    Graph,
}

/// 记忆分层类型（工作 / 短期 / 长期）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoType {
    Working,
    ShortTerm,
    LongTerm { kind: LongTermKind },
}

impl MemoType {
    /// 规范字符串形式，用于持久化。
    pub fn as_str(&self) -> String {
        match self {
            MemoType::Working => "working".to_string(),
            MemoType::ShortTerm => "short_term".to_string(),
            MemoType::LongTerm { kind } => match kind {
                LongTermKind::Episodic => "long_term:episodic".to_string(),
                LongTermKind::Semantic => "long_term:semantic".to_string(),
                LongTermKind::Entity => "long_term:entity".to_string(),
                LongTermKind::Graph => "long_term:graph".to_string(),
            },
        }
    }

}

impl std::str::FromStr for MemoType {
    type Err = MemoError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "working" => Ok(MemoType::Working),
            "short_term" => Ok(MemoType::ShortTerm),
            "long_term:episodic" => Ok(MemoType::LongTerm {
                kind: LongTermKind::Episodic,
            }),
            "long_term:semantic" => Ok(MemoType::LongTerm {
                kind: LongTermKind::Semantic,
            }),
            "long_term:entity" => Ok(MemoType::LongTerm {
                kind: LongTermKind::Entity,
            }),
            "long_term:graph" => Ok(MemoType::LongTerm {
                kind: LongTermKind::Graph,
            }),
            _ => Err(MemoError::InvalidParam(format!("unknown memo_type: {s}"))),
        }
    }
}

/// 一条记忆。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    pub id: MemoId,
    pub memo_type: MemoType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
    /// 重要性权重，取值 [0, 1]。
    pub importance: f32,
    pub version: u64,
    /// unix 秒。
    pub created_at: i64,
    pub updated_at: i64,
}

impl Memo {
    /// 校验字段合法性：拒绝空内容、越界重要性与空嵌入。
    pub fn validate(&self) -> Result<()> {
        if self.content.trim().is_empty() {
            return Err(MemoError::EmptyContent);
        }
        if !(0.0..=1.0).contains(&self.importance) {
            return Err(MemoError::InvalidParam(format!(
                "importance {} out of [0,1]",
                self.importance
            )));
        }
        if let Some(emb) = &self.embedding {
            if emb.is_empty() {
                return Err(MemoError::EmptyEmbedding);
            }
        }
        Ok(())
    }
}

/// 检索查询。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub top_k: usize,
    /// 语义权重 [0,1]。
    pub semantic_weight: f32,
    /// 关键词权重 [0,1]。
    pub keyword_weight: f32,
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
    /// 预计算查询向量（由 Manager 注入，供存储层做语义打分）。
    pub query_embedding: Option<Vec<f32>>,
}

impl SearchQuery {
    /// 构造带合理默认值的查询。
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            top_k: 10,
            semantic_weight: 0.7,
            keyword_weight: 0.3,
            score_threshold: 0.0,
            memo_type: None,
            query_embedding: None,
        }
    }

    /// 校验：拒绝空文本、top_k=0、权重越界与双零权重。
    pub fn validate(&self) -> Result<()> {
        if self.text.trim().is_empty() {
            return Err(MemoError::InvalidParam("empty query text".into()));
        }
        if self.top_k == 0 {
            return Err(MemoError::InvalidParam("top_k must be > 0".into()));
        }
        if !(0.0..=1.0).contains(&self.semantic_weight) {
            return Err(MemoError::InvalidParam("semantic_weight out of [0,1]".into()));
        }
        if !(0.0..=1.0).contains(&self.keyword_weight) {
            return Err(MemoError::InvalidParam("keyword_weight out of [0,1]".into()));
        }
        if self.semantic_weight == 0.0 && self.keyword_weight == 0.0 {
            return Err(MemoError::InvalidParam(
                "at least one of semantic_weight/keyword_weight must be > 0".into(),
            ));
        }
        Ok(())
    }
}

/// 带分数的检索结果。
#[derive(Debug, Clone)]
pub struct ScoredMemo {
    pub memo: Memo,
    pub score: f32,
}

/// 记忆更新补丁。
#[derive(Debug, Clone, Default)]
pub struct MemoPatch {
    pub content: Option<String>,
    pub memo_type: Option<MemoType>,
    pub metadata: Option<HashMap<String, String>>,
    pub importance: Option<f32>,
}

impl MemoPatch {
    pub fn is_empty(&self) -> bool {
        self.content.is_none()
            && self.memo_type.is_none()
            && self.metadata.is_none()
            && self.importance.is_none()
    }
}

/// 关键词重叠得分：查询词在内容中的命中比例（CJK 退化为子串匹配）。
pub fn keyword_score(content: &str, query: &str) -> f32 {
    let cl = content.to_lowercase();
    let ql = query.to_lowercase();
    let words: Vec<&str> = ql
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    if words.is_empty() {
        return if cl.contains(ql.trim()) { 1.0 } else { 0.0 };
    }
    let matched = words.iter().filter(|w| cl.contains(*w)).count();
    matched as f32 / words.len() as f32
}

/// 生成进程内唯一记忆 id。
pub fn generate_id() -> MemoId {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let c = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("m{nanos:x}{c:x}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Memo {
        Memo {
            id: "m1".into(),
            memo_type: MemoType::LongTerm {
                kind: LongTermKind::Episodic,
            },
            content: "hello world".into(),
            embedding: Some(vec![0.1, 0.2, 0.3]),
            metadata: HashMap::new(),
            importance: 0.5,
            version: 1,
            created_at: 100,
            updated_at: 100,
        }
    }

    #[test]
    fn memo_type_roundtrip() {
        for t in [
            MemoType::Working,
            MemoType::ShortTerm,
            MemoType::LongTerm {
                kind: LongTermKind::Episodic,
            },
            MemoType::LongTerm {
                kind: LongTermKind::Semantic,
            },
            MemoType::LongTerm {
                kind: LongTermKind::Entity,
            },
            MemoType::LongTerm {
                kind: LongTermKind::Graph,
            },
        ] {
            let s = t.as_str();
            assert_eq!(<MemoType as std::str::FromStr>::from_str(&s).unwrap(), t);
        }
    }

    #[test]
    fn memo_type_invalid() {
        assert!(<MemoType as std::str::FromStr>::from_str("bogus").is_err());
    }

    #[test]
    fn memo_validate_normal_and_abnormal() {
        assert!(sample().validate().is_ok());

        let mut m = sample();
        m.content = "   ".into();
        assert!(matches!(m.validate(), Err(MemoError::EmptyContent)));

        let mut m = sample();
        m.importance = 1.5;
        assert!(matches!(m.validate(), Err(MemoError::InvalidParam(_))));

        let mut m = sample();
        m.embedding = Some(vec![]);
        assert!(matches!(m.validate(), Err(MemoError::EmptyEmbedding)));
    }

    #[test]
    fn query_defaults_and_validate() {
        let q = SearchQuery::new("rust");
        assert_eq!(q.top_k, 10);
        assert!(q.validate().is_ok());

        let q = SearchQuery::new("");
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));

        let mut q = SearchQuery::new("x");
        q.top_k = 0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));

        let mut q = SearchQuery::new("x");
        q.semantic_weight = 0.0;
        q.keyword_weight = 0.0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));

        let mut q = SearchQuery::new("x");
        q.semantic_weight = 2.0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));
    }

    #[test]
    fn keyword_score_behaves() {
        assert_eq!(keyword_score("hello world", "hello world"), 1.0);
        assert_eq!(keyword_score("hello world", "hello rust"), 0.5);
        assert_eq!(keyword_score("hello world", "nope"), 0.0);
        // CJK 子串回退
        assert_eq!(keyword_score("用户喜欢 Rust", "Rust"), 1.0);
    }

    #[test]
    fn generate_id_unique() {
        let a = generate_id();
        let b = generate_id();
        assert_ne!(a, b);
    }
}
