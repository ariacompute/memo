use memory::MemoryManager;
use memory_core::{MemoryType, Result, SearchQuery};
use std::collections::HashMap;

/// 新增记忆并返回 id。
pub fn add(manager: &MemoryManager, mem_type: &str, content: &str, importance: f32) -> Result<String> {
    let mt = <MemoryType as std::str::FromStr>::from_str(mem_type)?;
    let id = manager.add(content, mt, HashMap::new(), importance)?;
    Ok(id)
}

/// 按 id 获取记忆（JSON），不存在返回 "not found"。
pub fn get(manager: &MemoryManager, id: &str) -> Result<String> {
    match manager.get(&id.to_string())? {
        Some(m) => Ok(serde_json::to_string_pretty(&m).unwrap_or_else(|_| "{}".to_string())),
        None => Ok("not found".to_string()),
    }
}

/// 混合检索，返回逐行 `score\tcontent`。
pub fn search(manager: &MemoryManager, text: &str, top_k: usize) -> Result<String> {
    let mut q = SearchQuery::new(text);
    q.top_k = top_k;
    let rs = manager.search(q)?;
    let lines: Vec<String> = rs
        .iter()
        .map(|r| format!("{:.3}\t{}", r.score, r.memory.content))
        .collect();
    Ok(lines.join("\n"))
}

/// 列出记忆（可按类型过滤）。
pub fn list(manager: &MemoryManager, mem_type: Option<&str>) -> Result<String> {
    let mt = mem_type.map(<MemoryType as std::str::FromStr>::from_str).transpose()?;
    let ms = manager.list(mt)?;
    let lines: Vec<String> = ms
        .iter()
        .map(|m| format!("{} [{:?}] {}", m.id, m.memory_type, m.content))
        .collect();
    Ok(lines.join("\n"))
}

/// 遗忘记忆，返回 "forgotten" 或 "not found"。
pub fn forget(manager: &MemoryManager, id: &str) -> Result<String> {
    Ok(if manager.forget(&id.to_string())? {
        "forgotten".to_string()
    } else {
        "not found".to_string()
    })
}

/// 进程内微基准：add / search，输出 JSON（供 `benches/` Python 解析）。
pub fn bench(manager: &MemoryManager, size: usize, top_k: usize, warmup: usize) -> Result<String> {
    if size == 0 {
        return Err(memory_core::MemoryError::InvalidParam(
            "bench size must be > 0".into(),
        ));
    }
    if top_k == 0 {
        return Err(memory_core::MemoryError::InvalidParam(
            "bench top_k must be > 0".into(),
        ));
    }

    for i in 0..warmup {
        let _ = manager.add(
            &format!("warmup memory content {i}"),
            MemoryType::Working,
            HashMap::new(),
            0.5,
        )?;
    }

    let mut add_ms: Vec<f64> = Vec::with_capacity(size);
    for i in 0..size {
        let content = format!(
            "bench item {i}: user prefers rust systems programming and local-first memory {i}"
        );
        let t0 = std::time::Instant::now();
        manager.add(&content, MemoryType::Working, HashMap::new(), 0.5)?;
        add_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let queries = [
        "rust systems programming",
        "local-first memory",
        "user prefers",
        "bench item",
        "programming",
    ];
    let mut search_ms: Vec<f64> = Vec::with_capacity(size);
    for i in 0..size {
        let qtext = queries[i % queries.len()];
        let mut q = SearchQuery::new(qtext);
        q.top_k = top_k;
        let t0 = std::time::Instant::now();
        let _ = manager.search(q)?;
        search_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let add_sum: f64 = add_ms.iter().sum();
    let search_sum: f64 = search_ms.iter().sum();
    let report = serde_json::json!({
        "system": "aria-memory",
        "includes_network": false,
        "offline": true,
        "size": size,
        "top_k": top_k,
        "warmup": warmup,
        "add": {
            "p50_ms": percentile(&mut add_ms, 0.50),
            "p99_ms": percentile(&mut add_ms, 0.99),
            "ops_per_sec": if add_sum > 0.0 { (size as f64) / (add_sum / 1000.0) } else { 0.0 },
        },
        "search": {
            "p50_ms": percentile(&mut search_ms, 0.50),
            "p99_ms": percentile(&mut search_ms, 0.99),
            "ops_per_sec": if search_sum > 0.0 { (size as f64) / (search_sum / 1000.0) } else { 0.0 },
        },
    });
    Ok(report.to_string())
}

fn percentile(xs: &mut [f64], p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((xs.len() as f64 - 1.0) * p).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use memory_embed::LocalEmbedder;
    use memory_storage::SqliteStore;
    use std::sync::Arc;

    fn mgr() -> MemoryManager {
        let e: Arc<dyn memory_core::Embedder> = Arc::new(LocalEmbedder::new(64));
        let s = SqliteStore::open(":memory:").unwrap();
        MemoryManager::new(e, Arc::new(s))
    }

    #[test]
    fn cli_add_search_get_forget() {
        let m = mgr();
        let id = add(&m, "working", "user likes rust", 0.8).unwrap();
        let out = search(&m, "rust", 5).unwrap();
        assert!(out.contains("rust"));
        let got = get(&m, &id).unwrap();
        assert!(got.contains("user likes rust"));
        assert_eq!(forget(&m, &id).unwrap(), "forgotten");
        assert_eq!(forget(&m, &id).unwrap(), "not found");
    }

    #[test]
    fn cli_invalid_type_errors() {
        let m = mgr();
        assert!(add(&m, "bogus", "x", 0.5).is_err());
    }

    #[test]
    fn cli_list_filters() {
        let m = mgr();
        add(&m, "working", "alpha", 0.5).unwrap();
        add(&m, "short_term", "beta", 0.5).unwrap();
        let all = list(&m, None).unwrap();
        assert!(all.contains("alpha") && all.contains("beta"));
        let filtered = list(&m, Some("working")).unwrap();
        assert!(filtered.contains("alpha") && !filtered.contains("beta"));
    }

    #[test]
    fn cli_bench_json_smoke() {
        let m = mgr();
        let out = bench(&m, 8, 3, 1).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["system"], "aria-memory");
        assert!(v["add"]["p50_ms"].as_f64().unwrap() >= 0.0);
        assert!(v["search"]["ops_per_sec"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn cli_bench_rejects_zero_size() {
        let m = mgr();
        assert!(bench(&m, 0, 5, 0).is_err());
    }
}
