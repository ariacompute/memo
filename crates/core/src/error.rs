use thiserror::Error;

/// 统一结果类型。
pub type Result<T> = std::result::Result<T, MemoryError>;

/// 记忆存储统一错误。任何失败都必须显式返回，禁止静默吞错。
#[derive(Debug, Error)]
pub enum MemoryError {
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("database error: {0}")]
    Db(String),

    #[error("memory not found: {0}")]
    NotFound(String),

    #[error("duplicate memory id: {0}")]
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

impl Clone for MemoryError {
    fn clone(&self) -> Self {
        match self {
            MemoryError::Io(e) => MemoryError::Io(std::io::Error::new(e.kind(), e.to_string())),
            MemoryError::Db(s) => MemoryError::Db(s.clone()),
            MemoryError::NotFound(s) => MemoryError::NotFound(s.clone()),
            MemoryError::DuplicateId(s) => MemoryError::DuplicateId(s.clone()),
            MemoryError::EmptyContent => MemoryError::EmptyContent,
            MemoryError::EmptyEmbedding => MemoryError::EmptyEmbedding,
            MemoryError::InvalidParam(s) => MemoryError::InvalidParam(s.clone()),
            MemoryError::Serialization(s) => MemoryError::Serialization(s.clone()),
            MemoryError::Embedding(s) => MemoryError::Embedding(s.clone()),
            MemoryError::Other(s) => MemoryError::Other(s.clone()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_messages() {
        assert_eq!(MemoryError::EmptyContent.to_string(), "empty content");
        assert_eq!(
            MemoryError::NotFound("x1".into()).to_string(),
            "memory not found: x1"
        );
        assert_eq!(
            MemoryError::DuplicateId("x2".into()).to_string(),
            "duplicate memory id: x2"
        );
        assert!(MemoryError::Db("boom".into())
            .to_string()
            .contains("boom"));
    }

    #[test]
    fn io_from_conversion() {
        let e: MemoryError = std::io::Error::other("disk").into();
        match e {
            MemoryError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn invalid_param_carries_reason() {
        let e = MemoryError::InvalidParam("top_k=0".into());
        assert!(e.to_string().contains("top_k=0"));
    }
}
