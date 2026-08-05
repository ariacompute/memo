//! memo: 长期记忆管理编排层，串接存储与嵌入，提供记忆的增删改查、检索、巩固、去重与遗忘。

pub mod lifecycle;
pub mod manager;

pub use lifecycle::{decay_importance, prune};
pub use manager::MemoManager;
