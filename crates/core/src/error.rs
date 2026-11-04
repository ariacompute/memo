use thiserror::Error;

/// Unified result type.
pub type Result<T> = std::result::Result<T, MemoError>;

/// Unified memory storage error. Any failure must be returned explicitly; silent swallowing is forbidden.
#[derive(Debug, Error)]
pub enum MemoError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("memo not found: {0}")]
    NotFound(String),

    #[error("duplicate memo id: {0}")]
    DuplicateId(String),

    #[error("empty content")]
    EmptyContent,

    #[error("empty embedding")]
    EmptyEmbedding,

    #[error("invalid parameter: {0}")]
    InvalidParam(String),

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("other error: {0}")]
    Other(String),
}

impl Clone for MemoError {
    fn clone(&self) -> Self {
        match self {
            MemoError::Io(e) => MemoError::Io(std::io::Error::new(e.kind(), e.to_string())),
            MemoError::Db(s) => MemoError::Db(s.clone()),
            MemoError::NotFound(s) => MemoError::NotFound(s.clone()),
            MemoError::DuplicateId(s) => MemoError::DuplicateId(s.clone()),
            MemoError::EmptyContent => MemoError::EmptyContent,
            MemoError::EmptyEmbedding => MemoError::EmptyEmbedding,
            MemoError::InvalidParam(s) => MemoError::InvalidParam(s.clone()),
            MemoError::Serialization(s) => MemoError::Serialization(s.clone()),
            MemoError::Embedding(s) => MemoError::Embedding(s.clone()),
            MemoError::Other(s) => MemoError::Other(s.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages() {
        assert_eq!(MemoError::EmptyContent.to_string(), "empty content");
        assert_eq!(
            MemoError::NotFound("x1".into()).to_string(),
            "memo not found: x1"
        );
        assert_eq!(
            MemoError::DuplicateId("x2".into()).to_string(),
            "duplicate memo id: x2"
        );
        assert!(MemoError::Db("boom".into()).to_string().contains("boom"));
    }

    #[test]
    fn io_from_conversion() {
        let e: MemoError = std::io::Error::other("disk").into();
        assert!(matches!(e, MemoError::Io(_)));
    }

    #[test]
    fn clone_preserves_every_variant() {
        let variants = vec![
            MemoError::Io(std::io::Error::new(std::io::ErrorKind::TimedOut, "slow")),
            MemoError::Db("db".into()),
            MemoError::NotFound("nf".into()),
            MemoError::DuplicateId("dup".into()),
            MemoError::EmptyContent,
            MemoError::EmptyEmbedding,
            MemoError::InvalidParam("bad".into()),
            MemoError::Serialization("ser".into()),
            MemoError::Embedding("emb".into()),
            MemoError::Other("other".into()),
        ];
        for e in variants {
            let cloned = e.clone();
            // Same variant and same Display text, and the io kind survives the clone.
            assert_eq!(cloned.to_string(), e.to_string());
            if let (MemoError::Io(a), MemoError::Io(b)) = (&e, &cloned) {
                assert_eq!(a.kind(), b.kind());
            }
        }
    }

    #[test]
    fn display_covers_every_variant() {
        assert_eq!(MemoError::EmptyEmbedding.to_string(), "empty embedding");
        assert!(MemoError::Serialization("s".into())
            .to_string()
            .contains("serialization error: s"));
        assert!(MemoError::Embedding("e".into())
            .to_string()
            .contains("embedding error: e"));
        assert!(MemoError::Other("o".into())
            .to_string()
            .contains("other error: o"));
        assert!(MemoError::Io(std::io::Error::other("disk"))
            .to_string()
            .contains("io error: disk"));
    }

    #[test]
    fn invalid_param_carries_reason() {
        let e = MemoError::InvalidParam("top_k=0".into());
        assert!(e.to_string().contains("top_k=0"));
    }
}
