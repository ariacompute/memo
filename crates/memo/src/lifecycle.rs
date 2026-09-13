use memo_core::*;

/// Importance decays exponentially by half-life (inspired by MemOS lifecycle management).
pub fn decay_importance(m: &mut Memo, elapsed_sec: i64, half_life_sec: i64) {
    if half_life_sec <= 0 || elapsed_sec <= 0 {
        return;
    }
    let factor = 0.5f64.powf(elapsed_sec as f64 / half_life_sec as f64);
    let v = m.importance as f64 * factor;
    m.importance = if v < 0.0 { 0.0 } else { v as f32 };
}

/// Forget memories whose importance is below the floor; returns the number removed.
pub fn prune(store: &dyn MemoStore, floor: f32) -> Result<usize> {
    if !(0.0..=1.0).contains(&floor) {
        return Err(MemoError::InvalidParam("floor out of [0,1]".into()));
    }
    let all = store.list(None)?;
    let mut n = 0;
    for m in all {
        if m.importance < floor && store.forget(&m.id)? {
            n += 1;
        }
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use memo_storage::SqliteStore;
    use std::collections::HashMap;

    fn sample(id: &str, importance: f32) -> Memo {
        Memo {
            id: id.into(),
            memo_type: MemoType::LongTerm {
                kind: LongTermKind::Episodic,
            },
            content: "x".into(),
            embedding: Some(vec![0.1]),
            metadata: HashMap::new(),
            importance,
            version: 1,
            created_at: 0,
            updated_at: 0,
        }
    }

    #[test]
    fn decay_halves_at_half_life() {
        let mut m = sample("a", 1.0);
        decay_importance(&mut m, 100, 100);
        assert!((m.importance - 0.5).abs() < 1e-6);
        // Zero elapsed time means no decay
        decay_importance(&mut m, 0, 100);
        assert!((m.importance - 0.5).abs() < 1e-6);
    }

    #[test]
    fn prune_removes_low_importance() {
        let s = SqliteStore::open(":memory:").unwrap();
        s.add(&sample("a", 0.1)).unwrap();
        s.add(&sample("b", 0.9)).unwrap();
        let n = prune(&s, 0.3).unwrap();
        assert_eq!(n, 1);
        assert!(s.get(&"a".into()).unwrap().is_none());
        assert!(s.get(&"b".into()).unwrap().is_some());
    }

    #[test]
    fn decay_reduces_and_skips_invalid() {
        let mut m = sample("a", 1.0);
        // a quarter-life of elapsed -> factor 0.5^0.5 ≈ 0.707
        decay_importance(&mut m, 100, 200);
        assert!(
            m.importance < 1.0 && m.importance > 0.5,
            "importance={}",
            m.importance
        );
        // non-positive half-life or elapsed -> no-op
        let before = m.importance;
        decay_importance(&mut m, 100, 0);
        decay_importance(&mut m, -10, 100);
        decay_importance(&mut m, 0, 100);
        assert!((m.importance - before).abs() < 1e-9);
    }

    #[test]
    fn prune_invalid_floor_and_noop() {
        let s = SqliteStore::open(":memory:").unwrap();
        assert!(matches!(prune(&s, 2.0), Err(MemoError::InvalidParam(_))));
        s.add(&sample("a", 0.9)).unwrap();
        s.add(&sample("b", 0.9)).unwrap();
        // floor below all importances -> nothing removed
        assert_eq!(prune(&s, 0.5).unwrap(), 0);
        assert_eq!(s.list(None).unwrap().len(), 2);
    }
}
