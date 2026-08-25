use memo_core::{Embedder, MemoError, Result};
use std::collections::HashMap;

/// Local lightweight embedder: maps text to a fixed-length vector.
///
/// Uses the hashing trick: stable-hashes word 1~2-grams and character 2-grams,
/// accumulates term frequency (TF), then applies L2 normalization.
/// Similar texts yield close vectors and can be compared directly with cosine similarity.
/// Depends on no external model or network, making it suitable for edge/mobile;
/// the `Embedder` trait allows swapping in a local small model.
pub struct LocalEmbedder {
    dim: usize,
}

impl LocalEmbedder {
    pub fn new(dim: usize) -> Self {
        Self { dim: dim.max(1) }
    }

    fn vectorize(&self, text: &str) -> Result<Vec<f32>> {
        let toks = tokenize(text);
        if toks.is_empty() {
            return Err(MemoError::EmptyEmbedding);
        }
        let mut vec = vec![0.0f32; self.dim];
        let mut counts: HashMap<usize, f32> = HashMap::new();
        for t in &toks {
            let h = hash_dim(t, self.dim);
            *counts.entry(h).or_insert(0.0) += 1.0;
        }
        let max = counts.values().cloned().fold(1.0f32, f32::max);
        for (h, c) in counts {
            // Normalize term frequency (TF) to keep long-text vectors from dominating.
            vec[h] = (c / max).sqrt();
        }
        let norm = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm == 0.0 {
            return Err(MemoError::EmptyEmbedding);
        }
        for v in vec.iter_mut() {
            *v /= norm;
        }
        Ok(vec)
    }
}

impl Embedder for LocalEmbedder {
    fn embed(&self, text: &str) -> Result<Vec<f32>> {
        self.vectorize(text)
    }

    fn dim(&self) -> usize {
        self.dim
    }
}

/// Tokenize: lowercase word 1~2-grams (English) and character 2-grams (CJK-compatible).
fn tokenize(text: &str) -> Vec<String> {
    let lower = text.to_lowercase();
    let mut toks: Vec<String> = Vec::new();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    for w in &words {
        toks.push((*w).to_string());
    }
    for pair in words.windows(2) {
        toks.push(format!("{} {}", pair[0], pair[1]));
    }
    let chars: Vec<char> = lower.chars().filter(|c| c.is_alphanumeric()).collect();
    for pair in chars.windows(2) {
        toks.push(pair.iter().collect());
    }
    toks
}

/// FNV-1a stable hash mapped into [0, dim).
fn hash_dim(s: &str, dim: usize) -> usize {
    let mut h: u64 = 0xcbf29ce484222325;
    for b in s.bytes() {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    (h as usize) % dim
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cosine;

    #[test]
    fn deterministic_and_dim() {
        let e = LocalEmbedder::new(64);
        let a = e.embed("the quick brown fox").unwrap();
        let b = e.embed("the quick brown fox").unwrap();
        assert_eq!(a.len(), 64);
        assert_eq!(a, b);
    }

    #[test]
    fn similar_text_close_vectors() {
        let e = LocalEmbedder::new(128);
        let a = e.embed("user prefers rust programming language").unwrap();
        let b = e.embed("user likes rust programming language").unwrap();
        let c = e.embed("banana smoothie recipe with ice").unwrap();
        let ab = cosine::cosine(&a, &b).unwrap();
        let ac = cosine::cosine(&a, &c).unwrap();
        assert!(ab > ac, "similar texts should score higher than dissimilar");
    }

    #[test]
    fn cjk_text_embedds() {
        let e = LocalEmbedder::new(64);
        let v = e.embed("用户喜欢 Rust 编程").unwrap();
        assert_eq!(v.len(), 64);
        assert!(v.iter().any(|x| *x != 0.0));
    }

    #[test]
    fn empty_text_is_error() {
        let e = LocalEmbedder::new(64);
        assert!(matches!(e.embed("   "), Err(MemoError::EmptyEmbedding)));
        assert!(matches!(e.embed("!!! ???"), Err(MemoError::EmptyEmbedding)));
    }
}
