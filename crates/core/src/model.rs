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
            keyword_weight: 0.5,
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

// ---------------------------------------------------------------------------
// Multi-relational memory plane (Jev-Mem inspired: semantic/temporal/causal/entity)
// ---------------------------------------------------------------------------

/// The four relational graph views. Each relation edge is tagged with exactly one kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationKind {
    /// Conceptual / meaning association.
    Semantic,
    /// Temporal ordering or co-occurrence.
    Temporal,
    /// Causal chain.
    Causal,
    /// Same entity / object aggregation.
    Entity,
}

impl RelationKind {
    /// Canonical string form used for persistence and wire transfer.
    pub fn as_str(&self) -> &'static str {
        match self {
            RelationKind::Semantic => "semantic",
            RelationKind::Temporal => "temporal",
            RelationKind::Causal => "causal",
            RelationKind::Entity => "entity",
        }
    }
}

impl std::str::FromStr for RelationKind {
    type Err = MemoError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "semantic" => Ok(RelationKind::Semantic),
            "temporal" => Ok(RelationKind::Temporal),
            "causal" => Ok(RelationKind::Causal),
            "entity" => Ok(RelationKind::Entity),
            _ => Err(MemoError::InvalidParam(format!("unknown relation_kind: {s}"))),
        }
    }
}

/// A directed relation edge between two memories, stored in its own table so that
/// `Memo` itself is never mutated.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub from_id: MemoId,
    pub to_id: MemoId,
    pub kind: RelationKind,
    /// Edge confidence in [0, 1].
    pub score: f32,
    /// Provenance description (e.g. "local:semantic" or "llm:causal").
    pub provenance: String,
    /// Unix timestamp in seconds.
    pub created_at: i64,
}

impl Relation {
    /// Validate: rejects self-loops, negative/over-range scores, and empty provenance.
    pub fn validate(&self) -> Result<()> {
        if self.from_id == self.to_id {
            return Err(MemoError::InvalidParam(format!(
                "relation self-loop rejected: {}",
                self.from_id
            )));
        }
        if !(0.0..=1.0).contains(&self.score) {
            return Err(MemoError::InvalidParam(format!(
                "relation score {} out of [0,1]",
                self.score
            )));
        }
        if self.provenance.trim().is_empty() {
            return Err(MemoError::InvalidParam("empty relation provenance".into()));
        }
        Ok(())
    }
}

/// A bounded graph retrieval query (mirrors Jev-Mem's Retrieve->Assess->Expand).
#[derive(Debug, Clone)]
pub struct GraphRetrieveQuery {
    /// Seed memory ids to start expansion from.
    pub seeds: Vec<MemoId>,
    /// Relation views to traverse; empty means "all four views".
    pub views: Vec<RelationKind>,
    /// Maximum number of distinct memories to visit (budget).
    pub budget: usize,
    /// Maximum traversal depth.
    pub max_hops: usize,
    /// Final number of scored memories to return.
    pub top_k: usize,
}

impl GraphRetrieveQuery {
    pub fn new(seeds: Vec<MemoId>) -> Self {
        Self {
            seeds,
            views: Vec::new(),
            budget: 60,
            max_hops: 3,
            top_k: 20,
        }
    }

    /// Validate the query: seeds and all bounds must be positive.
    pub fn validate(&self) -> Result<()> {
        if self.seeds.is_empty() {
            return Err(MemoError::InvalidParam("graph query needs >= 1 seed".into()));
        }
        if self.budget == 0 {
            return Err(MemoError::InvalidParam("graph budget must be > 0".into()));
        }
        if self.max_hops == 0 {
            return Err(MemoError::InvalidParam("graph max_hops must be > 0".into()));
        }
        if self.top_k == 0 {
            return Err(MemoError::InvalidParam("graph top_k must be > 0".into()));
        }
        Ok(())
    }
}

/// Inspectable decision trace returned alongside a graph retrieval, mirroring
/// Jev-Mem's typed/transparent decisions.
#[derive(Debug, Clone)]
pub struct RetrieveTrace {
    pub views: Vec<RelationKind>,
    pub budget: usize,
    pub stop_reason: String,
    pub hits: usize,
}

/// Bundled result of a bounded graph expansion.
#[derive(Debug, Clone)]
pub struct GraphRetrieveResult {
    pub items: Vec<ScoredMemo>,
    pub trace: RetrieveTrace,
}

/// Generic bounded graph traversal (BFS with cycle avoidance) shared by every
/// backend. `neighbor_fn` returns outgoing edges `(to_id, score, kind)` for a node;
/// the caller filters by `views`, caps visited nodes at `budget`, and decays the
/// accumulated score by 0.9 per hop. Returns reached `(id, score)` pairs plus a
/// human-readable stop reason.
pub fn graph_bfs(
    seeds: &[MemoId],
    views: &[RelationKind],
    budget: usize,
    max_hops: usize,
    mut neighbor_fn: impl FnMut(&MemoId) -> Vec<(MemoId, f32, RelationKind)>,
) -> (Vec<(MemoId, f32)>, String) {
    use std::collections::{HashMap, HashSet, VecDeque};

    let view_set: Option<&[RelationKind]> = if views.is_empty() { None } else { Some(views) };
    let mut best: HashMap<MemoId, f32> = HashMap::new();
    let mut visited: HashSet<MemoId> = HashSet::new();
    let mut queue: VecDeque<(MemoId, usize, f32)> = VecDeque::new();

    for s in seeds {
        if visited.insert(s.clone()) {
            best.insert(s.clone(), 1.0);
            queue.push_back((s.clone(), 0, 1.0));
        }
    }

    let mut stop = "exhausted".to_string();
    while let Some((node, hop, acc)) = queue.pop_front() {
        if hop >= max_hops {
            continue;
        }
        let edges = neighbor_fn(&node);
        for (nid, escore, kind) in edges {
            if let Some(vs) = view_set {
                if !vs.contains(&kind) {
                    continue;
                }
            }
            if visited.contains(&nid) {
                continue;
            }
            if visited.len() >= budget {
                stop = "budget".to_string();
                break;
            }
            let nacc = acc * escore * 0.9;
            visited.insert(nid.clone());
            let e = best.entry(nid.clone()).or_insert(0.0);
            if nacc > *e {
                *e = nacc;
            }
            queue.push_back((nid, hop + 1, nacc));
        }
        if stop == "budget" {
            break;
        }
    }

    let mut out: Vec<(MemoId, f32)> = best.into_iter().collect();
    out.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    (out, stop)
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

/// Stronger offline lexical relevance for hybrid retrieval (BM25-lite + exact
/// phrase bonus). Tokenizes into word tokens (len >= 2) and CJK character
/// 2-grams, rewards matched *specific* terms (rarity-weighted) and exact phrase
/// presence, normalized to `[0,1]`. Dependency-free, so it lifts retrieval
/// quality when the local embedding is weak. `keyword_score` stays the simpler
/// ratio (used by relation-scoring fallbacks); this is what the storage-layer
/// hybrid `search` uses.
pub fn lexical_relevance(content: &str, query: &str) -> f32 {
    let cl = content.to_lowercase();
    let ql = query.to_lowercase();
    let q_terms = tokenize_terms(&ql);
    if q_terms.is_empty() {
        return if ql.trim().is_empty() {
            0.0
        } else {
            f32::from(cl.contains(ql.trim()))
        };
    }
    // term frequencies within the content
    let c_terms = tokenize_terms(&cl);
    let mut cf: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for t in &c_terms {
        *cf.entry(t.as_str()).or_insert(0) += 1;
    }
    let mut sum = 0.0f32;
    for qt in &q_terms {
        if let Some(&tf) = cf.get(qt.as_str()) {
            // rarity weight: rarer terms are more discriminative; occurrence term
            // saturates so a term repeated many times does not dominate.
            let rarity = 1.0 / (1.0 + (tf as f32).ln());
            let occ = ((tf as f32).min(3.0) / 3.0).min(1.0);
            sum += rarity * (0.5 + 0.5 * occ);
        }
    }
    let mut score = sum / q_terms.len() as f32;
    // exact phrase bonus: a direct substring match is a strong relevance signal.
    let qtrim = ql.trim();
    if !qtrim.is_empty() && cl.contains(qtrim) {
        score = (score + 0.5).min(1.0);
    }
    score.min(1.0)
}

/// Tokenizer for lexical scoring: lowercase word tokens (len >= 2) plus CJK
/// character 2-grams. CJK text has no whitespace word tokens, so character
/// bigrams carry the lexical signal.
fn tokenize_terms(s: &str) -> Vec<String> {
    let sl = s.to_lowercase();
    let mut out: Vec<String> = Vec::new();
    for w in sl.split(|c: char| !c.is_alphanumeric()) {
        if w.len() >= 2 {
            out.push(w.to_string());
        }
    }
    let chars: Vec<char> = sl.chars().filter(|c| c.is_alphanumeric()).collect();
    let mut i = 0;
    while i + 1 < chars.len() {
        if is_cjk(chars[i]) && is_cjk(chars[i + 1]) {
            out.push(format!("{}{}", chars[i], chars[i + 1]));
        }
        i += 1;
    }
    out
}

/// Whether a character is in the CJK Unified Ideographs block.
fn is_cjk(c: char) -> bool {
    (c as u32) >= 0x4E00 && (c as u32) <= 0x9FFF
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

    #[test]
    fn lexical_relevance_behaves() {
        // exact phrase present -> strong score
        assert!(lexical_relevance("my name is martin", "my name is martin") > 0.9);
        // a specific rare term match must beat unrelated text
        let a = lexical_relevance("my name is martin and i like rust", "who is martin");
        let b = lexical_relevance("banana smoothie recipe with ice", "who is martin");
        assert!(a > b, "specific-term match must outrank unrelated text");
        assert!(a > 0.3 && b == 0.0);
        // CJK char-bigram overlap
        let c = lexical_relevance("用户喜欢编程", "用户编程");
        assert!(c > 0.0);
        // empty query -> 0
        assert_eq!(lexical_relevance("anything", ""), 0.0);
    }
}

#[cfg(test)]
mod relation_tests {
    use super::*;

    #[test]
    fn relation_kind_roundtrip() {
        for k in [
            RelationKind::Semantic,
            RelationKind::Temporal,
            RelationKind::Causal,
            RelationKind::Entity,
        ] {
            let s = k.as_str();
            assert_eq!(<RelationKind as std::str::FromStr>::from_str(s).unwrap(), k);
        }
        assert!(<RelationKind as std::str::FromStr>::from_str("bogus").is_err());
    }

    #[test]
    fn relation_validate_normal_and_abnormal() {
        let r = Relation {
            from_id: "a".into(),
            to_id: "b".into(),
            kind: RelationKind::Semantic,
            score: 0.7,
            provenance: "local:semantic".into(),
            created_at: 1,
        };
        assert!(r.validate().is_ok());

        let mut self_loop = r.clone();
        self_loop.to_id = "a".into();
        assert!(matches!(
            self_loop.validate(),
            Err(MemoError::InvalidParam(_))
        ));

        let mut bad_score = r.clone();
        bad_score.score = 1.5;
        assert!(matches!(
            bad_score.validate(),
            Err(MemoError::InvalidParam(_))
        ));

        let mut no_prov = r.clone();
        no_prov.provenance = "  ".into();
        assert!(matches!(
            no_prov.validate(),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn graph_query_validate() {
        let q = GraphRetrieveQuery::new(vec!["a".into()]);
        assert!(q.validate().is_ok());

        assert!(matches!(
            GraphRetrieveQuery::new(vec![]).validate(),
            Err(MemoError::InvalidParam(_))
        ));
        let mut q = GraphRetrieveQuery::new(vec!["a".into()]);
        q.budget = 0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));
        let mut q = GraphRetrieveQuery::new(vec!["a".into()]);
        q.max_hops = 0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));
        let mut q = GraphRetrieveQuery::new(vec!["a".into()]);
        q.top_k = 0;
        assert!(matches!(q.validate(), Err(MemoError::InvalidParam(_))));
    }

    #[test]
    fn graph_bfs_traverses_and_avoids_cycles() {
        // a -> b -> c, and c -> a (cycle). Seed at a, hop=2 budget=10.
        let edges: HashMap<MemoId, Vec<(MemoId, f32, RelationKind)>> = {
            let mut m = HashMap::new();
            m.insert(
                "a".into(),
                vec![("b".into(), 0.9, RelationKind::Semantic)],
            );
            m.insert(
                "b".into(),
                vec![("c".into(), 0.8, RelationKind::Semantic)],
            );
            m.insert("c".into(), vec![("a".into(), 0.7, RelationKind::Causal)]);
            m
        };
        let (out, stop) = graph_bfs(
            &["a".into()],
            &[],
            10,
            2,
            |n| edges.get(n).cloned().unwrap_or_default(),
        );
        let ids: Vec<&MemoId> = out.iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&&"a".to_string()));
        assert!(ids.contains(&&"b".to_string()));
        assert!(ids.contains(&&"c".to_string()));
        // Cycle must not cause infinite expansion; visited dedupes a.
        assert!(stop == "exhausted" || stop == "budget");
        // Seed keeps highest score.
        assert_eq!(out[0].0, "a");
    }

    #[test]
    fn graph_bfs_respects_view_filter_and_budget() {
        let edges: HashMap<MemoId, Vec<(MemoId, f32, RelationKind)>> = {
            let mut m = HashMap::new();
            m.insert(
                "a".into(),
                vec![
                    ("b".into(), 0.9, RelationKind::Semantic),
                    ("c".into(), 0.9, RelationKind::Temporal),
                ],
            );
            m
        };
        // Only semantic view -> c (temporal) excluded.
        let (out, _) = graph_bfs(
            &["a".into()],
            &[RelationKind::Semantic],
            10,
            3,
            |n| edges.get(n).cloned().unwrap_or_default(),
        );
        let ids: Vec<&MemoId> = out.iter().map(|(id, _)| id).collect();
        assert!(ids.contains(&&"b".to_string()));
        assert!(!ids.contains(&&"c".to_string()));

        // Budget=2 -> only seed + one neighbor.
        let (out, stop) = graph_bfs(
            &["a".into()],
            &[],
            2,
            3,
            |n| edges.get(n).cloned().unwrap_or_default(),
        );
        assert_eq!(out.len(), 2);
        assert_eq!(stop, "budget");
    }
}
