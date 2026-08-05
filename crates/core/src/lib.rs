//! memo-core: 共享记忆数据模型、统一错误与 trait 定义。
//! 本 crate 不依赖任何平台专属或重型 ML 依赖，作为分层架构的基座。

pub mod error;
pub mod model;
pub mod traits;

pub use error::{MemoError, Result};
pub use model::*;
pub use traits::*;

use std::time::{SystemTime, UNIX_EPOCH};

/// 当前 unix 秒（失败时回退为 0）。
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
