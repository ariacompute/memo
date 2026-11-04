//! Shared helpers for CLI unit tests (`#[cfg(test)]` only).
//!
//! The CLI reads process-wide state (environment variables, the home directory)
//! and writes files; tests that touch any of it serialize on [`env_lock`] so
//! `cargo test` stays parallel-safe and deterministic.

use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

/// Serialize tests that mutate `ARIA_COMPUTE_HOME` / `ARIA_MEMO_SITE` /
/// `ARIA_MEMO_UPGRADE_URL`. Poisoning is recovered so one failing test does not
/// cascade into unrelated failures.
pub(crate) fn env_lock() -> MutexGuard<'static, ()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
        .lock()
        .expect("env lock poisoned by a panicking test")
}

/// A unique, empty temp directory for the given test tag.
pub(crate) fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("aria-memo-cli-{}-{tag}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}
