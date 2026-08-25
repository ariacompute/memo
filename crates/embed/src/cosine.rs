use memo_core::{MemoError, Result};

/// Cosine similarity. Errors if either vector is empty or dimensions mismatch;
/// returns 0 (no direction) only when a vector's magnitude is 0.
pub fn cosine(a: &[f32], b: &[f32]) -> Result<f32> {
    if a.is_empty() || b.is_empty() {
        return Err(MemoError::EmptyEmbedding);
    }
    if a.len() != b.len() {
        return Err(MemoError::InvalidParam(format!(
            "embedding dim mismatch: {} vs {}",
            a.len(),
            b.len()
        )));
    }
    let dot = a.iter().zip(b).map(|(x, y)| x * y).sum::<f32>();
    let na = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let nb = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if na == 0.0 || nb == 0.0 {
        return Ok(0.0);
    }
    Ok(dot / (na * nb))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn identical_is_one() {
        let v = vec![0.5, 0.5, 0.0];
        assert!((cosine(&v, &v).unwrap() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn orthogonal_is_zero() {
        let a = vec![1.0, 0.0];
        let b = vec![0.0, 1.0];
        assert!(cosine(&a, &b).unwrap().abs() < 1e-6);
    }

    #[test]
    fn dim_mismatch_errors() {
        assert!(matches!(
            cosine(&[1.0], &[1.0, 0.0]),
            Err(MemoError::InvalidParam(_))
        ));
    }

    #[test]
    fn empty_errors() {
        assert!(matches!(cosine(&[], &[1.0]), Err(MemoError::EmptyEmbedding)));
    }
}
