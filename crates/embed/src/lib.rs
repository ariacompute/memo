//! memory-embed: 本地轻量嵌入（ngram + 哈希/TF-IDF 向量）与余弦相似度，零外部 ML 依赖。

pub mod cosine;
pub mod local;

pub use cosine::cosine;
pub use local::LocalEmbedder;
