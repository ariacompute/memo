//! memo-embed: local lightweight embeddings (ngram + hashed/TF-IDF vectors) and cosine
//! similarity, with zero external ML dependencies.

pub mod cosine;
pub mod local;

pub use cosine::cosine;
pub use local::LocalEmbedder;
