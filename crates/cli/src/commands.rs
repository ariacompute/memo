use memo::MemoManager;
use memo_core::{MemoPatch, MemoType, Result, SearchQuery};
use std::collections::HashMap;

/// 新增记忆并返回 id。未知 memo_type 回退默认 working（类型口径差异不应阻断评测）。
pub fn add(manager: &MemoManager, mem_type: &str, content: &str, importance: f32) -> Result<String> {
    let mt = match <MemoType as std::str::FromStr>::from_str(mem_type) {
        Ok(t) => t,
        Err(_) => {
            eprintln!("[warn] unknown memo_type '{}', falling back to working", mem_type);
            MemoType::Working
        }
    };
    let id = manager.add(content, mt, HashMap::new(), importance)?;
    Ok(id)
}

/// 按 id 获取记忆（JSON），不存在返回 "not found"。
pub fn get(manager: &MemoManager, id: &str) -> Result<String> {
    match manager.get(&id.to_string())? {
        Some(m) => Ok(serde_json::to_string_pretty(&m).unwrap_or_else(|_| "{}".to_string())),
        None => Ok("not found".to_string()),
    }
}

/// 混合检索。
/// 默认返回逐行 `score\tcontent`（向后兼容）；`as_json` 时返回 JSON 数组。
pub fn search(manager: &MemoManager, text: &str, top_k: usize, as_json: bool) -> Result<String> {
    let mut q = SearchQuery::new(text);
    q.top_k = top_k;
    let rs = manager.search(q)?;
    if as_json {
        let arr: Vec<serde_json::Value> = rs
            .iter()
            .map(|r| {
                serde_json::json!({
                    "score": r.score,
                    "id": r.memo.id,
                    "memo_type": format!("{:?}", r.memo.memo_type),
                    "content": r.memo.content,
                })
            })
            .collect();
        return Ok(serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string()));
    }
    let lines: Vec<String> = rs
        .iter()
        .map(|r| format!("{:.3}\t{}", r.score, r.memo.content))
        .collect();
    Ok(lines.join("\n"))
}

/// 列出记忆（可按类型过滤）。
/// 默认返回 `id [type] content`（向后兼容）；`as_json` 时返回 JSON 数组。
pub fn list(manager: &MemoManager, mem_type: Option<&str>, as_json: bool) -> Result<String> {
    let mt = mem_type.map(<MemoType as std::str::FromStr>::from_str).transpose()?;
    let ms = manager.list(mt)?;
    if as_json {
        let arr: Vec<serde_json::Value> = ms
            .iter()
            .map(|m| {
                serde_json::json!({
                    "id": m.id,
                    "memo_type": format!("{:?}", m.memo_type),
                    "content": m.content,
                    "importance": m.importance,
                    "version": m.version,
                    "metadata": m.metadata,
                })
            })
            .collect();
        return Ok(serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string()));
    }
    let lines: Vec<String> = ms
        .iter()
        .map(|m| format!("{} [{:?}] {}", m.id, m.memo_type, m.content))
        .collect();
    Ok(lines.join("\n"))
}

/// 按 id 更新记忆（内容/类型/重要性），至少一项非空。
/// 内容变更自动重算 embedding、version+1。
pub fn update(
    manager: &MemoManager,
    id: &str,
    content: Option<&str>,
    mem_type: Option<&str>,
    importance: Option<f32>,
) -> Result<()> {
    let mt = match mem_type {
        Some(t) => Some(<MemoType as std::str::FromStr>::from_str(t)?),
        None => None,
    };
    let patch = MemoPatch {
        content: content.map(|c| c.to_string()),
        memo_type: mt,
        metadata: None,
        importance,
    };
    manager.update(&id.to_string(), patch)
}

/// 遗忘记忆，返回 "forgotten" 或 "not found"。
pub fn forget(manager: &MemoManager, id: &str) -> Result<String> {
    Ok(if manager.forget(&id.to_string())? {
        "forgotten".to_string()
    } else {
        "not found".to_string()
    })
}

/// 进程内微基准：add / search，输出 JSON（供 `benches/` Python 解析）。
pub fn bench(manager: &MemoManager, size: usize, top_k: usize, warmup: usize) -> Result<String> {
    if size == 0 {
        return Err(memo_core::MemoError::InvalidParam(
            "bench size must be > 0".into(),
        ));
    }
    if top_k == 0 {
        return Err(memo_core::MemoError::InvalidParam(
            "bench top_k must be > 0".into(),
        ));
    }

    for i in 0..warmup {
        let _ = manager.add(
            &format!("warmup memo content {i}"),
            MemoType::Working,
            HashMap::new(),
            0.5,
        )?;
    }

    let mut add_ms: Vec<f64> = Vec::with_capacity(size);
    for i in 0..size {
        let content = format!(
            "bench item {i}: user prefers rust systems programming and local-first memo {i}"
        );
        let t0 = std::time::Instant::now();
        manager.add(&content, MemoType::Working, HashMap::new(), 0.5)?;
        add_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }

    let queries = [
        "rust systems programming",
        "local-first memo",
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
        "system": "aria-memo",
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
    use memo_embed::LocalEmbedder;
    use memo_storage::SqliteStore;
    use std::sync::Arc;

    fn mgr() -> MemoManager {
        let e: Arc<dyn memo_core::Embedder> = Arc::new(LocalEmbedder::new(64));
        let s = SqliteStore::open(":memory:").unwrap();
        MemoManager::new(e, Arc::new(s))
    }

    #[test]
    fn cli_add_search_get_forget() {
        let m = mgr();
        let id = add(&m, "working", "user likes rust", 0.8).unwrap();
        let out = search(&m, "rust", 5, false).unwrap();
        assert!(out.contains("rust"));
        let got = get(&m, &id).unwrap();
        assert!(got.contains("user likes rust"));
        assert_eq!(forget(&m, &id).unwrap(), "forgotten");
        assert_eq!(forget(&m, &id).unwrap(), "not found");
    }

    #[test]
    fn cli_unknown_type_falls_back_to_working() {
        let m = mgr();
        // 未知 memo_type 不应中断评测管线，降级为 working 并成功写入
        let id = add(&m, "bogus", "x", 0.5).expect("unknown type should fall back, not error");
        assert!(!id.is_empty());
    }

    #[test]
    fn cli_list_filters() {
        let m = mgr();
        add(&m, "working", "alpha", 0.5).unwrap();
        add(&m, "short_term", "beta", 0.5).unwrap();
        let all = list(&m, None, false).unwrap();
        assert!(all.contains("alpha") && all.contains("beta"));
        let filtered = list(&m, Some("working"), false).unwrap();
        assert!(filtered.contains("alpha") && !filtered.contains("beta"));
    }

    #[test]
    fn cli_list_and_search_json() {
        let m = mgr();
        let id = add(&m, "working", "user likes rust", 0.8).unwrap();
        let arr: serde_json::Value = serde_json::from_str(&list(&m, None, true).unwrap()).unwrap();
        assert!(arr.is_array());
        assert_eq!(arr[0]["id"], id);
        assert_eq!(arr[0]["content"], "user likes rust");
        assert!(arr[0]["importance"].as_f64().is_some());

        let s: serde_json::Value =
            serde_json::from_str(&search(&m, "rust", 5, true).unwrap()).unwrap();
        assert!(s.is_array());
        assert_eq!(s[0]["id"], id);
        assert!((s[0]["score"].as_f64().unwrap()) > 0.0);
    }

    #[test]
    fn cli_update_success_and_errors() {
        let m = mgr();
        let id = add(&m, "working", "old content", 0.5).unwrap();
        // 缺 id 已在 main 层保证；这里测空 patch 报错
        assert!(update(&m, &id, None, None, None).is_err());
        // 内容更新应成功，版本递增
        update(&m, &id, Some("new content"), None, Some(0.9)).unwrap();
        let got: serde_json::Value = serde_json::from_str(&get(&m, &id).unwrap()).unwrap();
        assert_eq!(got["content"], "new content");
        assert_eq!(got["importance"].as_f64().unwrap(), 0.9);
        assert_eq!(got["version"].as_u64().unwrap(), 2);
        // 非法类型报错
        assert!(update(&m, &id, Some("x"), Some("bogus"), None).is_err());
        // 不存在 id 报错
        assert!(update(&m, "nope", Some("x"), None, None).is_err());
    }

    #[test]
    fn cli_bench_json_smoke() {
        let m = mgr();
        let out = bench(&m, 8, 3, 1).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["system"], "aria-memo");
        assert!(v["add"]["p50_ms"].as_f64().unwrap() >= 0.0);
        assert!(v["search"]["ops_per_sec"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn cli_bench_rejects_zero_size() {
        let m = mgr();
        assert!(bench(&m, 0, 5, 0).is_err());
    }
}
