use memo_core::{Memo, MemoError, MemoId, Result, StorageBackend};

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

impl std::str::FromStr for BackendKind {
    type Err = MemoError;

    fn from_str(s: &str) -> Result<Self> {
        match s {
            "embedded" => Ok(BackendKind::Embedded),
            "replicated" => Ok(BackendKind::Replicated),
            _ => Err(MemoError::InvalidParam(format!(
                "unknown backend kind: {s}"
            ))),
        }
    }
}

/// Replicated/distributed backend placeholder (M1 only abstracts it, inspired by rqlite).
/// A real implementation would replicate memories across edge nodes via Raft; deferred to a later milestone.
pub struct ReplicatedBackend;

fn not_implemented() -> MemoError {
    MemoError::Other("replicated backend not implemented in M1".into())
}

impl ReplicatedBackend {
    pub fn new() -> Self {
        Self
    }

    /// Unimplemented in M1: returns an `Other` error when called.
    pub fn open(_location: &str) -> Result<Self> {
        Err(not_implemented())
    }
}

impl Default for ReplicatedBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl StorageBackend for ReplicatedBackend {
    fn open(location: &str) -> Result<Self> {
        // Fully qualified: the inherent constructor is the single source of truth.
        ReplicatedBackend::open(location)
    }
    fn migrate(&self) -> Result<()> {
        Err(not_implemented())
    }
    fn backend_kind(&self) -> &'static str {
        BackendKind::Replicated.as_str()
    }
    fn put_record(&self, _memo: &Memo) -> Result<()> {
        Err(not_implemented())
    }
    fn get_record(&self, _id: &MemoId) -> Result<Option<Memo>> {
        Err(not_implemented())
    }
    fn delete_record(&self, _id: &MemoId) -> Result<bool> {
        Err(not_implemented())
    }
    fn scan_records(&self) -> Result<Vec<Memo>> {
        Err(not_implemented())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn replicated_backend_not_implemented() {
        assert!(ReplicatedBackend::open("any").is_err());
        assert_eq!(
            ReplicatedBackend::new().backend_kind(),
            BackendKind::Replicated.as_str()
        );
    }

    #[test]
    fn replicated_backend_raw_crud_fails_closed() {
        let rb = ReplicatedBackend::new();
        let memo = Memo {
            id: "a".into(),
            memo_type: memo_core::MemoType::Working,
            content: "x".into(),
            embedding: None,
            metadata: Default::default(),
            importance: 0.5,
            version: 1,
            created_at: 1,
            updated_at: 1,
        };
        // Every raw-CRUD entry point must report `Other` instead of a silent no-op.
        assert!(matches!(rb.put_record(&memo), Err(MemoError::Other(_))));
        assert!(matches!(
            rb.get_record(&"a".to_string()),
            Err(MemoError::Other(_))
        ));
        assert!(matches!(
            rb.delete_record(&"a".to_string()),
            Err(MemoError::Other(_))
        ));
        assert!(matches!(rb.scan_records(), Err(MemoError::Other(_))));
        // The trait constructor fails closed too.
        assert!(<ReplicatedBackend as StorageBackend>::open("any").is_err());
    }

    #[test]
    fn backend_kind_roundtrip() {
        for k in [BackendKind::Embedded, BackendKind::Replicated] {
            assert_eq!(BackendKind::from_str(k.as_str()).unwrap(), k);
        }
        assert!("bogus".parse::<BackendKind>().is_err());
    }

    #[test]
    fn embedded_kind_string_and_replicated_unimplemented() {
        assert_eq!(BackendKind::Embedded.as_str(), "embedded");
        assert_eq!(
            BackendKind::from_str("embedded").unwrap(),
            BackendKind::Embedded
        );
        // ReplicatedBackend is a placeholder: open/migrate error, but kind resolves.
        assert!(ReplicatedBackend::open("any").is_err());
        let rb = ReplicatedBackend::new();
        assert_eq!(rb.backend_kind(), BackendKind::Replicated.as_str());
        assert!(rb.migrate().is_err());
        // `Default` mirrors `new`.
        let dflt = <ReplicatedBackend as Default>::default();
        assert_eq!(dflt.backend_kind(), BackendKind::Replicated.as_str());
    }
}
