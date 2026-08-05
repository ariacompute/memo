use thiserror::Error;

/// 统一结果类型。
pub type Result<T> = std::result::Result<T, MemoError>;

/// 记忆存储统一错误。任何失败都必须显式返回，禁止静默吞错。
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
        assert!(MemoError::Db("boom".into())
            .to_string()
            .contains("boom"));
    }

    #[test]
    fn io_from_conversion() {
        let e: MemoError = std::io::Error::other("disk").into();
        match e {
            MemoError::Io(_) => {}
            other => panic!("expected Io, got {other:?}"),
        }
    }

    #[test]
    fn invalid_param_carries_reason() {
        let e = MemoError::InvalidParam("top_k=0".into());
        assert!(e.to_string().contains("top_k=0"));
    }
}
