//! memo-storage: embedded persistence backend (SQLite/rusqlite) responsible for
//! schema creation, migration, indexing, and CRUD.

pub mod backend;
pub mod sqlite;

pub use backend::{BackendKind, ReplicatedBackend};
pub use sqlite::SqliteStore;
