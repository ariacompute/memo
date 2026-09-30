//! memo: long-term memory orchestration layer that wires together storage and embedding,
//! providing memory CRUD, retrieval, consolidation, deduplication, and forgetting.

pub mod lifecycle;
pub mod manager;

pub use lifecycle::{decay_importance, prune};
pub use manager::{LocalRelationScorer, MemoManager, MemoryController, RelationConfig, RelationScorer};
// Re-export the multi-relational data model so callers depend only on `memo`.
pub use memo_core::{
    GraphRetrieveQuery, GraphRetrieveResult, Relation, RelationKind, RetrieveTrace,
};
