//! memo-storage: 嵌入式持久化后端（SQLite/rusqlite），负责建表、迁移、索引与 CRUD。

pub mod backend;
pub mod sqlite;

pub use backend::{BackendKind, ReplicatedBackend};
pub use sqlite::SqliteStore;
