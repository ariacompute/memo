use memo_core::{Embedder, MemoError, Result};
use std::collections::HashMap;

/// 本地轻量嵌入器：将文本映射为定长向量。
///
/// 采用 hashing trick：对词的 1~2 gram 与字符 2 gram 做稳定哈希并累加词频（TF），
/// 再做 L2 归一化。相似文本得到相近向量，可直接用余弦相似度比较。
/// 不依赖任何外部模型或网络，适合边缘/移动端；通过 `Embedder` trait 可替换为本地小模型。
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
            // 归一化词频（TF），避免长文本向量量级偏大。
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

/// 分词：小写化后的词 1~2 gram（英文）与字符 2 gram（兼容 CJK）。
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

/// FNV-1a 稳定哈希到 [0, dim)。
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
