use memo::{MemoryController, MemoManager};
use memo_core::{
    generate_id, now_secs, Memo, MemoError, MemoPatch, MemoType, RecallQuery, Relation,
    RelationKind, Result, SearchQuery,
};
use std::collections::HashMap;
use std::str::FromStr;

/// Add a memory and return its id. An unknown memo_type falls back to `working`
/// (type schema differences must not break the evaluation pipeline).
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

/// Fetch a memory by id (JSON); returns "not found" if absent.
pub fn get(manager: &MemoManager, id: &str) -> Result<String> {
    match manager.get(&id.to_string())? {
        Some(m) => Ok(serde_json::to_string_pretty(&m).unwrap_or_else(|_| "{}".to_string())),
        None => Ok("not found".to_string()),
    }
}

/// Hybrid search.
/// Defaults to per-line `score\tcontent` (for backward compatibility); returns a JSON array when `as_json`.
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

/// Pure vector (semantic) recall. Returns memories ranked by cosine similarity
/// to the query embedding (no keyword component). Output mirrors `search`.
pub fn recall(manager: &MemoManager, text: &str, top_k: usize, as_json: bool) -> Result<String> {
    let mut q = RecallQuery::new(text);
    q.top_k = top_k;
    let rs = manager.recall(q)?;
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

/// List memories (optionally filtered by type).
/// Defaults to `id [type] content` (for backward compatibility); returns a JSON array when `as_json`.
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

/// Update a memory by id (content/type/importance), at least one field must be set.
/// Content changes trigger embedding recomputation and increment the version.
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

/// Forget a memory, returning "forgotten" or "not found".
pub fn forget(manager: &MemoManager, id: &str) -> Result<String> {
    Ok(if manager.forget(&id.to_string())? {
        "forgotten".to_string()
    } else {
        "not found".to_string()
    })
}

/// Batch hybrid search over multiple queries (one per entry in `queries`).
pub fn search_batch(
    manager: &MemoManager,
    queries: &[String],
    top_k: usize,
    as_json: bool,
) -> Result<String> {
    let sqs: Vec<SearchQuery> = queries
        .iter()
        .map(|t| {
            let mut q = SearchQuery::new(t.clone());
            q.top_k = top_k;
            q
        })
        .collect();
    let results = manager.search_batch(&sqs)?;
    if as_json {
        let arr: Vec<serde_json::Value> = results
            .iter()
            .enumerate()
            .map(|(i, rs)| {
                serde_json::json!({
                    "query": queries[i],
                    "results": rs
                        .iter()
                        .map(|s| serde_json::json!({"id": s.memo.id, "score": s.score, "content": s.memo.content}))
                        .collect::<Vec<_>>(),
                })
            })
            .collect();
        return Ok(serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string()));
    }
    let lines: Vec<String> = results
        .iter()
        .enumerate()
        .flat_map(|(i, rs)| {
            let mut out = vec![format!("# query {}: {}", i, queries[i])];
            out.extend(rs.iter().map(|s| format!("{:.3}\t{}", s.score, s.memo.content)));
            out
        })
        .collect();
    Ok(lines.join("\n"))
}

/// In-process micro-benchmark: add / search, output as JSON (parsed by the `benches/` Python scripts).
///
/// Write-path segments (controlled by flags) let the harness compare the long write tail:
///  - `add_baseline`: default journal (DELETE) + per-item embed + per-item transaction.
///  - `add_wal`: same naive writes but after `enable_wal()` (WAL journal mode).
///  - `add_batch_embed`: `manager.add_batch` (batch embed + transactional batch write).
///  - `add_bulk`: per-item embed but `store.add_batch` (transaction merge only).
///
/// `BenchConfig` groups the run options so the `bench` signature stays small while
/// still exposing the write-tail comparison flags (WAL / batch-embed / bulk).
pub struct BenchConfig {
    pub size: usize,
    pub top_k: usize,
    pub warmup: usize,
    pub search_batch: bool,
    pub wal: bool,
    pub batch_embed: bool,
    pub bulk: bool,
}

/// Search segments (`search` / `batch`) are unchanged from before.
pub fn bench(manager: &MemoManager, cfg: &BenchConfig) -> Result<String> {
    let BenchConfig {
        size,
        top_k,
        warmup,
        search_batch,
        wal,
        batch_embed,
        bulk,
    } = *cfg;
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

    let bench_item = |i: usize, r: usize| -> String {
        format!("bench item {r}-{i}: user prefers rust systems programming and local-first memo {i}")
    };

    let mut report = serde_json::json!({
        "system": "aria-memo",
        "includes_network": false,
        "offline": true,
        "size": size,
        "top_k": top_k,
        "warmup": warmup,
    });

    // Baseline: default journal + per-item embed + per-item transaction.
    let mut add_ms: Vec<f64> = Vec::with_capacity(size);
    reset_corpus(manager);
    for i in 0..size {
        let content = bench_item(i, 0);
        let t0 = std::time::Instant::now();
        manager.add(&content, MemoType::Working, HashMap::new(), 0.5)?;
        add_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
    }
    report["add_baseline"] = add_segment(&mut add_ms);

    // WAL: enable WAL, then naive per-item writes (isolates journal-mode effect).
    if wal {
        manager.store().enable_wal()?;
        let mut wal_ms: Vec<f64> = Vec::with_capacity(size);
        reset_corpus(manager);
        for i in 0..size {
            let content = bench_item(i, 1);
            let t0 = std::time::Instant::now();
            manager.add(&content, MemoType::Working, HashMap::new(), 0.5)?;
            wal_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
        }
        report["add_wal"] = add_segment(&mut wal_ms);
    }

    // Batch-embed: embed all at once, then a single transactional batch write.
    if batch_embed {
        let reps = warmup + 1;
        let mut per_add: Vec<f64> = Vec::with_capacity(reps);
        for r in 0..reps {
            reset_corpus(manager);
            let contents: Vec<String> = (0..size).map(|i| bench_item(i, r)).collect();
            let t0 = std::time::Instant::now();
            manager.add_batch(&contents, MemoType::Working, 0.5)?;
            per_add.push(t0.elapsed().as_secs_f64() * 1000.0 / size as f64);
        }
        report["add_batch_embed"] = add_segment(&mut per_add);
    }

    // Bulk: per-item embed but merge into one transaction via `store.add_batch`.
    if bulk {
        let reps = warmup + 1;
        let mut per_add: Vec<f64> = Vec::with_capacity(reps);
        for r in 0..reps {
            reset_corpus(manager);
            let now = now_secs();
            let mut memos: Vec<Memo> = Vec::with_capacity(size);
            for i in 0..size {
                let content = bench_item(i, r);
                let emb = manager.embedder().embed(&content)?;
                memos.push(Memo {
                    id: generate_id(),
                    memo_type: MemoType::Working,
                    content,
                    embedding: Some(emb),
                    metadata: HashMap::new(),
                    importance: 0.5,
                    version: 1,
                    created_at: now,
                    updated_at: now,
                });
            }
            let t0 = std::time::Instant::now();
            manager.store().add_batch(&memos)?;
            per_add.push(t0.elapsed().as_secs_f64() * 1000.0 / size as f64);
        }
        report["add_bulk"] = add_segment(&mut per_add);
    }

    // Search baseline (unchanged).
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
    report["search"] = add_segment(&mut search_ms);

    // Optional: batch retrieval throughput (single lock, all queries at once).
    if search_batch {
        let batch_queries: Vec<SearchQuery> = (0..size)
            .map(|i| {
                let mut q = SearchQuery::new(queries[i % queries.len()]);
                q.top_k = top_k;
                q
            })
            .collect();
        for _ in 0..warmup {
            let _ = manager.search_batch(&batch_queries);
        }
        let t0 = std::time::Instant::now();
        let _ = manager.search_batch(&batch_queries)?;
        let batch_ms = t0.elapsed().as_secs_f64() * 1000.0;
        report["batch"] = serde_json::json!({
            "total_ms": batch_ms,
            "ops_per_sec": if batch_ms > 0.0 { (size as f64) / (batch_ms / 1000.0) } else { 0.0 },
        });
    }
    Ok(report.to_string())
}

/// Summarize a vector of per-add latencies (ms) into p50/p99 and per-second throughput.
fn add_segment(xs: &mut [f64]) -> serde_json::Value {
    if xs.is_empty() {
        return serde_json::json!({ "p50_ms": 0.0, "p99_ms": 0.0, "ops_per_sec": 0.0 });
    }
    let sum: f64 = xs.iter().sum();
    let mean = sum / xs.len() as f64;
    serde_json::json!({
        "p50_ms": percentile(xs, 0.50),
        "p99_ms": percentile(xs, 0.99),
        "ops_per_sec": if mean > 0.0 { 1000.0 / mean } else { 0.0 },
    })
}

/// Clear all stored memories so each benchmark segment starts from an empty corpus.
fn reset_corpus(manager: &MemoManager) {
    if let Ok(all) = manager.list(None) {
        for m in all {
            let _ = manager.forget(&m.id);
        }
    }
}

fn percentile(xs: &mut [f64], p: f64) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let idx = ((xs.len() as f64 - 1.0) * p).round() as usize;
    xs[idx.min(xs.len() - 1)]
}

// ---------------------------------------------------------------------------
// Multi-relational (Jev-Mem) CLI commands
// ---------------------------------------------------------------------------

fn parse_views(spec: &str) -> Result<Vec<RelationKind>> {
    if spec.trim().is_empty() {
        return Ok(Vec::new());
    }
    spec.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(<RelationKind as FromStr>::from_str)
        .collect::<Result<Vec<_>>>()
}

/// Build a controller from a manager's embedder + store.
fn controller(manager: &MemoManager) -> MemoryController {
    MemoryController::with_defaults(manager.embedder(), manager.store())
}

/// Write→connect: infer four-view relations for an already-stored memory by id.
pub fn connect(manager: &MemoManager, id: &str) -> Result<String> {
    let m = manager
        .get(&id.to_string())?
        .ok_or_else(|| MemoError::NotFound(id.to_string()))?;
    let ctrl = controller(manager);
    let n = ctrl.connect(&m)?;
    Ok(format!("connected {id}: {n} edge(s)"))
}

/// Manually add a relation edge between two memories.
pub fn relate(
    manager: &MemoManager,
    from: &str,
    to: &str,
    kind: &str,
    score: f32,
    provenance: Option<&str>,
) -> Result<String> {
    if score < 0.0 || score > 1.0 {
        return Err(MemoError::InvalidParam("score out of [0,1]".into()));
    }
    let k = <RelationKind as FromStr>::from_str(kind)?;
    let rel = Relation {
        from_id: from.to_string(),
        to_id: to.to_string(),
        kind: k,
        score,
        provenance: provenance.unwrap_or("cli").to_string(),
        created_at: memo_core::now_secs(),
    };
    manager.store().add_relation(&rel)?;
    Ok(format!(
        "relation {}->{}:{} added (score {:.3})",
        from, to, k.as_str(), score
    ))
}

/// List relations, optionally filtered by from/to/kind.
pub fn relations(
    manager: &MemoManager,
    from: Option<&str>,
    to: Option<&str>,
    kind: Option<&str>,
    top_k: usize,
    as_json: bool,
) -> Result<String> {
    let k = match kind {
        Some(s) => Some(<RelationKind as FromStr>::from_str(s)?),
        None => None,
    };
    let rels = manager.store().get_relations(
        from.map(|s| s.to_string()).as_ref(),
        to.map(|s| s.to_string()).as_ref(),
        k,
        top_k,
    )?;
    if as_json {
        let arr: Vec<serde_json::Value> = rels
            .iter()
            .map(|r| {
                serde_json::json!({
                    "from_id": r.from_id,
                    "to_id": r.to_id,
                    "kind": r.kind.as_str(),
                    "score": r.score,
                    "provenance": r.provenance,
                    "created_at": r.created_at,
                })
            })
            .collect();
        return Ok(serde_json::to_string_pretty(&arr).unwrap_or_else(|_| "[]".to_string()));
    }
    let lines: Vec<String> = rels
        .iter()
        .map(|r| {
            format!(
                "{}->{}:{} {:.3} [{}]",
                r.from_id, r.to_id, r.kind.as_str(), r.score, r.provenance
            )
        })
        .collect();
    Ok(lines.join("\n"))
}

/// Retrieve→assess→expand: hybrid-seed a graph traversal and return scored
/// memories plus an inspectable trace. `views` is a comma-separated spec
/// (e.g. "semantic,entity"); empty means "all four views".
pub fn graph_retrieve(
    manager: &MemoManager,
    text: &str,
    views: &str,
    budget: usize,
    max_hops: usize,
    top_k: usize,
    as_json: bool,
) -> Result<String> {
    let views = parse_views(views)?;
    let ctrl = controller(manager);
    let res = ctrl.retrieve(text, views, budget, max_hops, top_k)?;
    if as_json {
        let items: Vec<serde_json::Value> = res
            .items
            .iter()
            .map(|s| {
                serde_json::json!({
                    "score": s.score,
                    "id": s.memo.id,
                    "content": s.memo.content,
                })
            })
            .collect();
        let trace = serde_json::json!({
            "views": res.trace.views.iter().map(|v| v.as_str()).collect::<Vec<_>>(),
            "budget": res.trace.budget,
            "stop_reason": res.trace.stop_reason,
            "hits": res.trace.hits,
        });
        let out = serde_json::json!({ "items": items, "trace": trace });
        return Ok(serde_json::to_string_pretty(&out).unwrap_or_else(|_| "{}".to_string()));
    }
    let lines: Vec<String> = res
        .items
        .iter()
        .map(|s| format!("{:.3}\t{}", s.score, s.memo.content))
        .collect();
    Ok(lines.join("\n"))
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
    fn cli_recall_returns_semantic_match() {
        let m = mgr();
        let rust_id = add(&m, "working", "user prefers rust for systems programming", 0.8).unwrap();
        add(&m, "working", "banana smoothie recipe with ice", 0.5).unwrap();
        let out = recall(&m, "rust programming language", 5, false).unwrap();
        assert!(out.contains("rust"));
        let arr: serde_json::Value =
            serde_json::from_str(&recall(&m, "rust programming language", 5, true).unwrap()).unwrap();
        assert_eq!(arr[0]["id"], rust_id);
    }

    #[test]
    fn cli_unknown_type_falls_back_to_working() {
        let m = mgr();
        // An unknown memo_type must not break the evaluation pipeline; degrade to working and write successfully
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
        // Missing id is guaranteed at the main layer; here we test that an empty patch errors
        assert!(update(&m, &id, None, None, None).is_err());
        // Content update should succeed and bump the version
        update(&m, &id, Some("new content"), None, Some(0.9)).unwrap();
        let got: serde_json::Value = serde_json::from_str(&get(&m, &id).unwrap()).unwrap();
        assert_eq!(got["content"], "new content");
        assert_eq!(got["importance"].as_f64().unwrap(), 0.9);
        assert_eq!(got["version"].as_u64().unwrap(), 2);
        // Invalid type errors
        assert!(update(&m, &id, Some("x"), Some("bogus"), None).is_err());
        // Non-existent id errors
        assert!(update(&m, "nope", Some("x"), None, None).is_err());
    }

    #[test]
    fn cli_bench_json_smoke() {
        let m = mgr();
        let cfg = BenchConfig {
            size: 8,
            top_k: 3,
            warmup: 1,
            search_batch: false,
            wal: false,
            batch_embed: false,
            bulk: false,
        };
        let out = bench(&m, &cfg).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        assert_eq!(v["system"], "aria-memo");
        assert!(v["add_baseline"]["p50_ms"].as_f64().unwrap() >= 0.0);
        assert!(v["search"]["ops_per_sec"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn cli_bench_rejects_zero_size() {
        let m = mgr();
        let cfg = BenchConfig {
            size: 0,
            top_k: 5,
            warmup: 0,
            search_batch: false,
            wal: false,
            batch_embed: false,
            bulk: false,
        };
        assert!(bench(&m, &cfg).is_err());
    }

    #[test]
    fn cli_bench_rejects_zero_topk() {
        let m = mgr();
        let cfg = BenchConfig {
            size: 5,
            top_k: 0,
            warmup: 0,
            search_batch: false,
            wal: false,
            batch_embed: false,
            bulk: false,
        };
        assert!(bench(&m, &cfg).is_err());
    }

    #[test]
    fn cli_bench_write_tail_segments_present() {
        let m = mgr();
        // enable all write-tail segments
        let cfg = BenchConfig {
            size: 8,
            top_k: 3,
            warmup: 1,
            search_batch: false,
            wal: true,
            batch_embed: true,
            bulk: true,
        };
        let out = bench(&m, &cfg).unwrap();
        let v: serde_json::Value = serde_json::from_str(&out).unwrap();
        // baseline always present; WAL/batch-embed/bulk reported when requested
        assert!(v["add_baseline"]["p99_ms"].as_f64().is_some());
        assert!(v["add_wal"]["p99_ms"].as_f64().is_some());
        assert!(v["add_batch_embed"]["p99_ms"].as_f64().is_some());
        assert!(v["add_bulk"]["p99_ms"].as_f64().is_some());
        // search segment untouched
        assert!(v["search"]["ops_per_sec"].as_f64().unwrap() > 0.0);
    }

    #[test]
    fn cli_get_not_found_and_search_empty() {
        let m = mgr();
        assert_eq!(get(&m, "ghost").unwrap(), "not found");
        // No memories -> search/recall return empty strings (not errors)
        assert_eq!(search(&m, "anything", 5, false).unwrap(), "");
        assert_eq!(recall(&m, "anything", 5, false).unwrap(), "");
        let arr: serde_json::Value =
            serde_json::from_str(&search(&m, "anything", 5, true).unwrap()).unwrap();
        assert!(arr.is_array() && arr.as_array().unwrap().is_empty());
    }

    #[test]
    fn cli_relate_list_and_graph() {
        let m = mgr();
        let a = add(&m, "working", "user likes rust systems programming", 0.8).unwrap();
        let b = add(&m, "working", "user likes rust language for systems", 0.8).unwrap();

        // Manual relate.
        let out = relate(&m, &a, &b, "semantic", 0.9, Some("cli")).unwrap();
        assert!(out.contains("added"));

        // List relations filtered by kind.
        let json = relations(&m, None, None, Some("semantic"), 10, true).unwrap();
        let arr: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(arr.as_array().unwrap().len(), 1);
        assert_eq!(arr[0]["from_id"], a);

        // Graph retrieval from text near b should reach a through the semantic edge.
        let g = graph_retrieve(&m, "rust programming", "", 20, 3, 10, true).unwrap();
        let v: serde_json::Value = serde_json::from_str(&g).unwrap();
        assert!(v["items"].as_array().unwrap().len() >= 2);
        assert!(!v["trace"]["stop_reason"].as_str().unwrap().is_empty());
        assert_eq!(v["trace"]["hits"].as_u64().unwrap(), 2);

        // Invalid kind rejected.
        assert!(relate(&m, &a, &b, "bogus", 0.9, None).is_err());
        // Invalid score rejected.
        assert!(relate(&m, &a, &b, "semantic", 1.5, None).is_err());
    }

    #[test]
    fn cli_connect_infers_edges() {
        let m = mgr();
        let _a = add(&m, "working", "user prefers rust systems programming", 0.8).unwrap();
        let b = add(&m, "working", "rust is used for systems programming by the user", 0.8).unwrap();
        // connect on b should infer edges to a (semantic/entity).
        let out = connect(&m, &b).unwrap();
        assert!(out.contains("edge"));
        let json = relations(&m, None, None, None, 50, true).unwrap();
        let arr: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert!(arr.as_array().unwrap().len() >= 2);
        // connect on missing id errors.
        assert!(connect(&m, "ghost").is_err());
    }

    #[test]
    fn cli_parse_views_helper() {
        assert!(parse_views("").unwrap().is_empty());
        assert_eq!(parse_views("semantic,temporal").unwrap().len(), 2);
        assert!(parse_views("nonsense").is_err());
    }
}
