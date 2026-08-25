use memo_core::{MemoError, Result, StorageBackend};

/// Backend kind identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    Embedded,
    Replicated,
}

impl BackendKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            BackendKind::Embedded => "embedded",
            BackendKind::Replicated => "replicated",
        }
    }
}

/// Replicated/distributed backend placeholder (M1 only abstracts it, inspired by rqlite).
/// A real implementation would replicate memories across edge nodes via Raft; deferred to a later milestone.
pub struct ReplicatedBackend;

impl ReplicatedBackend {
    pub fn new() -> Self {
        Self
    }

    /// Unimplemented in M1: returns an `Other` error when called.
    pub fn open(_location: &str) -> Result<()> {
        Err(MemoError::Other(
            "replicated backend not implemented in M1".into(),
        ))
    }
}

impl Default for ReplicatedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for ReplicatedBackend {
    fn migrate(&self) -> Result<()> {
        Err(MemoError::Other(
            "replicated backend not implemented in M1".into(),
        ))
    }
    fn backend_kind(&self) -> &'static str {
        BackendKind::Replicated.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replicated_backend_not_implemented() {
        assert!(ReplicatedBackend::open("any").is_err());
        assert_eq!(
            ReplicatedBackend::new().backend_kind(),
            BackendKind::Replicated.as_str()
        );
    }
}
