use memo_core::{MemoError, Result, StorageBackend};

/// 后端种类标识。
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

/// 复制/分布式后端占位（M1 仅做抽象，rqlite 灵感）。
/// 真实实现将基于 Raft 在多个边缘节点间复制记忆，列为后续里程碑。
pub struct ReplicatedBackend;

impl ReplicatedBackend {
    pub fn new() -> Self {
        Self
    }

    /// M1 未实现：调用即返回 `Other` 错误。
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
