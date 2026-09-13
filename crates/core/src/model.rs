use crate::error::{MemoError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Unique memory identifier.
pub type MemoId = String;

/// Long-term memory subtypes (inspired by mem0).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LongTermKind {
    Episodic,
    Semantic,
    Entity,
    Graph,
}

/// Memory tier types (working / short-term / long-term).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MemoType {
    Working,
    ShortTerm,
    LongTerm { kind: LongTermKind },
}

impl MemoType {
    /// Canonical string form, used for persistence.
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

/// A single memory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Memo {
    pub id: MemoId,
    pub memo_type: MemoType,
    pub content: String,
    pub embedding: Option<Vec<f32>>,
    pub metadata: HashMap<String, String>,
    /// Importance weight in the range [0, 1].
    pub importance: f32,
    pub version: u64,
    /// Unix timestamp in seconds.
    pub created_at: i64,
    pub updated_at: i64,
}

impl Memo {
    /// Validate field legality: rejects empty content, out-of-range importance, and empty embeddings.
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

/// A search query.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SearchQuery {
    pub text: String,
    pub top_k: usize,
    /// Semantic weight in [0,1].
    pub semantic_weight: f32,
    /// Keyword weight in [0,1].
    pub keyword_weight: f32,
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
    /// Precomputed query vector (injected by the Manager for semantic scoring in the storage layer).
    pub query_embedding: Option<Vec<f32>>,
}

impl SearchQuery {
    /// Construct a query with sensible defaults.
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

    /// Validate: rejects empty text, top_k=0, out-of-range weights, and both-zero weights.
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

/// A vector (semantic) recall query. Scoring is pure cosine similarity between
/// the query embedding and stored memory embeddings — no keyword component —
/// which is what distinguishes it from the hybrid [`SearchQuery`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecallQuery {
    pub text: String,
    pub top_k: usize,
    /// Minimum cosine similarity to keep a candidate.
    pub score_threshold: f32,
    pub memo_type: Option<MemoType>,
    /// Precomputed query vector (injected by the Manager so the storage layer
    /// does not need an embedder).
    pub query_embedding: Option<Vec<f32>>,
}

impl RecallQuery {
    /// Construct a query with sensible defaults.
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            top_k: 10,
            score_threshold: 0.0,
            memo_type: None,
            query_embedding: None,
        }
    }

    /// Validate: rejects empty text and top_k=0.
    pub fn validate(&self) -> Result<()> {
        if self.text.trim().is_empty() {
            return Err(MemoError::InvalidParam("empty recall text".into()));
        }
        if self.top_k == 0 {
            return Err(MemoError::InvalidParam("top_k must be > 0".into()));
        }
        Ok(())
    }
}

/// A search result with a score.
#[derive(Debug, Clone)]
pub struct ScoredMemo {
    pub memo: Memo,
    pub score: f32,
}

/// A memory update patch.
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

/// Keyword overlap score: ratio of query terms found in the content
/// (CJK falls back to substring matching).
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

/// Generate an in-process unique memory id.
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
        // CJK substring fallback
        assert_eq!(keyword_score("用户喜欢 Rust", "Rust"), 1.0);
    }

    #[test]
    fn generate_id_unique() {
        let a = generate_id();
        let b = generate_id();
        assert_ne!(a, b);
    }

    #[test]
    fn recall_query_defaults_and_validate() {
        let q = RecallQuery::new("rust");
        assert_eq!(q.top_k, 10);
        assert!(q.validate().is_ok());
        assert!(q.query_embedding.is_none());

        let q = RecallQuery::new("");
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));

        let mut q = RecallQuery::new("x");
        q.top_k = 0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));
    }

    #[test]
    fn memo_patch_is_empty() {
        assert!(MemoPatch::default().is_empty());
        assert!(
            !MemoPatch {
                content: Some("x".into()),
                ..Default::default()
            }
            .is_empty()
        );
        assert!(
            !MemoPatch {
                importance: Some(0.5),
                ..Default::default()
            }
            .is_empty()
        );
    }

    #[test]
    fn keyword_score_partial_and_zero() {
        // Two query words, only one present -> 0.5
        assert_eq!(keyword_score("hello world", "hello rust"), 0.5);
        // No overlapping words -> 0
        assert_eq!(keyword_score("apple pie", "rust go"), 0.0);
        // Empty content + non-empty query -> 0
        assert_eq!(keyword_score("", "x"), 0.0);
    }
}
