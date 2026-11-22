//! memo: long-term memory orchestration layer that wires together storage and embedding,
//! providing memory CRUD, retrieval, consolidation, deduplication, and forgetting.

pub mod lifecycle;
pub mod manager;

pub use lifecycle::{
    decay_importance, decay_importance_by_tier, half_life_for, prune, LONG_TERM_HALF_LIFE_SEC,
    SHORT_TERM_HALF_LIFE_SEC, WORKING_HALF_LIFE_SEC,
};
pub use manager::{
    LocalRelationScorer, MemoManager, MemoryController, RelationConfig, RelationScorer,
};
// Re-export the multi-relational data model so callers depend only on `memo`.
pub use memo_core::{
    GraphRetrieveQuery, GraphRetrieveResult, Relation, RelationKind, RetrieveTrace,
};
