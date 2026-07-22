use memory_core::*;

/// 重要性按半衰期指数衰减（参考 MemOS 生命周期管理）。
pub fn decay_importance(m: &mut Memory, elapsed_sec: i64, half_life_sec: i64) {
    if half_life_sec <= 0 || elapsed_sec <= 0 {
        return;
    }
    let factor = 0.5f64.powf(elapsed_sec as f64 / half_life_sec as f64);
    let v = m.importance as f64 * factor;
    m.importance = if v < 0.0 { 0.0 } else { v as f32 };
}

/// 删除重要性低于阈值的记忆，返回删除条数。
pub fn prune(store: &dyn MemoryStore, floor: f32) -> Result<usize> {
    if !(0.0..=1.0).contains(&floor) {
        return Err(MemoryError::InvalidParam("floor out of [0,1]".into()));
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
    use memory_storage::SqliteStore;
    use std::collections::HashMap;

    fn sample(id: &str, importance: f32) -> Memory {
        Memory {
            id: id.into(),
            memory_type: MemoryType::LongTerm {
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
        // 经过时间 0 不衰减
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
}
