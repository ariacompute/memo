//! memo-core: shared memory data models, unified errors, and trait definitions.
//! This crate depends on no platform-specific or heavy ML dependencies, serving as the base of the layered architecture.

pub mod error;
pub mod model;
pub mod traits;

pub use error::{MemoError, Result};
pub use model::*;
pub use traits::*;

use std::time::{SystemTime, UNIX_EPOCH};

/// Current unix seconds (falls back to 0 on failure).
pub fn now_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
