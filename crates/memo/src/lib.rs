//! memo: long-term memory orchestration layer that wires together storage and embedding,
//! providing memory CRUD, retrieval, consolidation, deduplication, and forgetting.

pub mod lifecycle;
pub mod manager;

pub use lifecycle::{decay_importance, prune};
pub use manager::MemoManager;
