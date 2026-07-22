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
}
