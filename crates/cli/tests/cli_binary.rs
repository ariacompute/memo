//! End-to-end tests for the real `aria-memo` binary.
//!
//! These run the compiled executable (not just `entry()`), which is the only
//! way to execute the process entry point `fn main` — including its
//! `std::process::exit` behaviour. Workflow tests use a file-backed SQLite DB
//! in a unique temp directory so every CLI invocation is a separate process
//! that still observes prior writes. All commands are offline and need no
//! external services or credentials.

use serde_json::Value;
use std::fs;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicUsize, Ordering};

fn bin() -> Command {
    Command::new(env!("CARGO_BIN_EXE_aria-memo"))
}

static NEXT_DB: AtomicUsize = AtomicUsize::new(0);

struct TempDb {
    dir: PathBuf,
    path: String,
}

impl TempDb {
    fn new(tag: &str) -> Self {
        let seq = NEXT_DB.fetch_add(1, Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("aria-memo-e2e-{}-{tag}-{seq}", std::process::id()));
        fs::create_dir_all(&dir).expect("create e2e temp dir");
        let path = dir.join("memo.db");
        Self {
            dir,
            path: path.to_string_lossy().into_owned(),
        }
    }

    fn run(&self, args: &[&str]) -> Output {
        bin()
            .arg("--db")
            .arg(&self.path)
            .args(args)
            .output()
            .expect("run aria-memo")
    }

    fn ok(&self, args: &[&str]) -> String {
        let out = self.run(args);
        assert!(
            out.status.success(),
            "args={args:?} status={:?} stderr={}",
            out.status,
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8(out.stdout).expect("utf-8 stdout")
    }

    fn json(&self, args: &[&str]) -> Value {
        serde_json::from_str(&self.ok(args)).expect("valid JSON stdout")
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.dir);
    }
}

fn stdout_text(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// Run the binary inside `dir` (so relative defaults like `./memo.db` stay in the
/// temp dir) with extra environment variables.
fn run_in(dir: &std::path::Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = bin();
    cmd.current_dir(dir).args(args);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output().expect("run aria-memo")
}

#[test]
fn version_exits_zero() {
    // `--version` prints the bare version number (clap's Version action).
    let out = bin().arg("--version").output().expect("run aria-memo");
    assert!(out.status.success(), "status: {:?}", out.status);
    let stdout = stdout_text(&out);
    assert!(
        stdout.chars().any(|c| c.is_ascii_digit()),
        "stdout: {stdout}"
    );

    // The explicit `version` subcommand prints the product name.
    let out = bin().arg("version").output().expect("run aria-memo");
    assert!(out.status.success());
    let stdout = stdout_text(&out);
    assert!(stdout.contains("aria-memo"), "stdout: {stdout}");
}

#[test]
fn runtime_error_exits_one_with_a_message_on_stderr() {
    let out = bin()
        .args([
            "--db",
            ":memory:",
            "update",
            "--id",
            "missing",
            "--content",
            "x",
        ])
        .output()
        .expect("run aria-memo");
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("error:"), "stderr: {stderr}");
}

#[test]
fn usage_error_exits_two() {
    let out = bin()
        .arg("--definitely-not-a-flag")
        .output()
        .expect("run aria-memo");
    assert_eq!(out.status.code(), Some(2));
}

#[test]
fn full_memory_lifecycle_across_processes() {
    let db = TempDb::new("lifecycle");

    let rust_id = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "user prefers rust for systems programming",
            "--importance",
            "0.9",
        ])
        .trim()
        .to_string();
    let food_id = db
        .ok(&[
            "add",
            "--type",
            "short_term",
            "--content",
            "banana smoothie recipe with ice",
            "--importance",
            "0.5",
        ])
        .trim()
        .to_string();
    assert!(!rust_id.is_empty() && !food_id.is_empty());
    assert_ne!(rust_id, food_id);

    let got = db.json(&["get", "--id", &rust_id]);
    assert_eq!(got["id"], rust_id);
    assert_eq!(got["content"], "user prefers rust for systems programming");

    let search = db.json(&["search", "--text", "rust", "--top-k", "5", "--json"]);
    assert!(search.is_array());
    assert!(search
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["id"] == rust_id));

    let recall = db.json(&[
        "recall",
        "--text",
        "rust programming",
        "--top-k",
        "5",
        "--json",
    ]);
    assert!(recall.is_array());
    assert_eq!(recall[0]["id"], rust_id);

    let listed = db.json(&["list", "--json"]);
    assert_eq!(listed.as_array().unwrap().len(), 2);

    let updated = db.ok(&[
        "update",
        "--id",
        &rust_id,
        "--content",
        "user loves rust even more",
        "--importance",
        "0.7",
    ]);
    assert!(updated.contains("updated"));
    let got = db.json(&["get", "--id", &rust_id]);
    assert_eq!(got["content"], "user loves rust even more");
    assert_eq!(got["version"], 2);
    assert!((got["importance"].as_f64().unwrap() - 0.7).abs() < 1e-6);

    assert_eq!(db.ok(&["forget", "--id", &rust_id]).trim(), "forgotten");
    assert_eq!(db.ok(&["forget", "--id", &rust_id]).trim(), "not found");
    assert_eq!(db.json(&["list", "--json"]).as_array().unwrap().len(), 1);
}

#[test]
fn aging_decays_importance_across_processes() {
    let db = TempDb::new("age");
    // An empty database ages nothing but still reports a count.
    assert_eq!(db.ok(&["age", "--elapsed-sec", "3600"]).trim(), "aged 0");
    let working = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "fresh working memory",
            "--importance",
            "1.0",
        ])
        .trim()
        .to_string();
    db.ok(&[
        "add",
        "--type",
        "long_term:semantic",
        "--content",
        "durable semantic memory",
        "--importance",
        "1.0",
    ]);

    // One Working half-life: the Working memory halves, the LongTerm one barely moves
    // but is still counted as aged.
    let aged = db.json(&["age", "--elapsed-sec", "3600", "--json"]);
    assert_eq!(aged["aged"], 2);
    assert_eq!(aged["elapsed_sec"], 3600);
    let got = db.json(&["get", "--id", &working]);
    assert!((got["importance"].as_f64().unwrap() - 0.5).abs() < 1e-6);
    assert_eq!(got["version"], 2);

    // Default human-readable output stays `aged <n>`.
    let human = db.ok(&["age", "--elapsed-sec", "3600"]);
    assert!(human.trim().starts_with("aged "), "stdout: {human}");

    // Zero elapsed is a runtime error (exit 1 + `error:` on stderr).
    let out = db.run(&["age", "--elapsed-sec", "0"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("error:"));
    // A negative value needs the `=` form (otherwise clap reads it as a flag and reports
    // a usage error); the runtime guard then rejects it instead of silently clamping.
    let out = db.run(&["age", "--elapsed-sec=-1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("error:"));
}

#[test]
fn search_batch_and_typed_list_across_processes() {
    let db = TempDb::new("batch");
    let rust = db
        .ok(&[
            "add",
            "--type",
            "short_term",
            "--content",
            "rust async runtime tokio",
            "--importance",
            "0.9",
        ])
        .trim()
        .to_string();
    let sql = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "sqlite fts5 bm25 indexing",
            "--importance",
            "0.7",
        ])
        .trim()
        .to_string();
    let food = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "banana smoothie with ice",
            "--importance",
            "0.2",
        ])
        .trim()
        .to_string();

    // `search-batch` keeps one result group per query, in argument order.
    let batch = db.json(&[
        "search-batch",
        "--text",
        "sqlite fts5 ranking",
        "--text",
        "banana smoothie",
        "--top-k",
        "2",
        "--json",
    ]);
    let groups = batch.as_array().expect("batch array");
    assert_eq!(groups.len(), 2);
    assert_eq!(groups[0]["query"], "sqlite fts5 ranking");
    assert!(groups[0]["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == sql.as_str()));
    assert_eq!(groups[1]["query"], "banana smoothie");
    assert!(groups[1]["results"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == food.as_str()));

    // `list --type` filters persisted rows, not just in-memory ones.
    let short = db.json(&["list", "--type", "short_term", "--json"]);
    let rows = short.as_array().unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0]["id"], rust.as_str());

    // `recall` is pure vector recall, so the semantically closest row must win.
    let recall = db.json(&[
        "recall",
        "--text",
        "rust tokio async",
        "--top-k",
        "1",
        "--json",
    ]);
    assert_eq!(recall[0]["id"], rust.as_str());
}

#[test]
fn bench_reports_segments_across_processes() {
    let db = TempDb::new("bench");
    let full = db.json(&[
        "bench", "--size", "4", "--warmup", "1", "--top-k", "2", "--json",
    ]);
    assert_eq!(full["system"], "aria-memo");
    assert_eq!(full["size"], 4);
    assert_eq!(full["offline"], true);
    assert!(full["add_baseline"]["p50_ms"].as_f64().unwrap() >= 0.0);
    assert!(full["search"]["ops_per_sec"].as_f64().unwrap() > 0.0);

    // `--no-baseline --batch` reports only the requested segments.
    let light = db.json(&[
        "bench",
        "--size",
        "4",
        "--warmup",
        "1",
        "--top-k",
        "2",
        "--batch",
        "--no-baseline",
        "--json",
    ]);
    assert!(light.get("add_baseline").is_none());
    assert!(light["batch"]["total_ms"].as_f64().unwrap() >= 0.0);
    assert!(light["search"]["p99_ms"].as_f64().unwrap() >= 0.0);

    // Invalid parameters are rejected before any write happens.
    let out = db.run(&["bench", "--size", "0"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("bench size must be > 0"));
}

#[test]
fn unicode_and_option_like_content_across_processes() {
    let db = TempDb::new("unicode");
    // `--content=<value>` keeps a leading dash from being parsed as a flag.
    let dash = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content=-dash-prefixed note about rust",
        ])
        .trim()
        .to_string();
    assert!(!dash.is_empty());
    let cjk = db
        .ok(&[
            "add",
            "--type",
            "long_term:semantic",
            "--content",
            "用户喜欢用 Rust 写系统程序",
        ])
        .trim()
        .to_string();

    // CJK segmentation feeds the lexical FTS5 branch end-to-end.
    let hits = db.json(&["search", "--text", "系统程序", "--top-k", "5", "--json"]);
    assert!(hits
        .as_array()
        .unwrap()
        .iter()
        .any(|h| h["id"] == cjk.as_str()));

    // `get` dumps the canonical record, so non-ASCII content round-trips exactly.
    let got = db.json(&["get", "--id", &cjk]);
    assert_eq!(got["content"], "用户喜欢用 Rust 写系统程序");
    assert_eq!(got["memo_type"]["LongTerm"]["kind"], "Semantic");

    // A missing id is a normal answer (exit 0 + `not found`), not a failure.
    let out = db.run(&["get", "--id", "missing"]);
    assert_eq!(out.status.code(), Some(0));
    assert_eq!(stdout_text(&out).trim(), "not found");
}

#[test]
fn default_database_path_is_created_in_cwd() {
    let db = TempDb::new("default-db");
    let add = run_in(
        &db.dir,
        &[
            "add",
            "--type",
            "working",
            "--content",
            "default path probe",
        ],
        &[],
    );
    assert!(
        add.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    // `--db` defaults to `./memo.db`, resolved against the process cwd.
    assert!(db.dir.join("memo.db").exists());

    let list = run_in(&db.dir, &["list", "--json"], &[]);
    assert!(list.status.success());
    assert!(stdout_text(&list).contains("default path probe"));
}

#[test]
fn setup_config_lifecycle_under_override_home() {
    let db = TempDb::new("setup");
    let home = db.dir.to_string_lossy().into_owned();
    let home_env: &[(&str, &str)] = &[("ARIA_COMPUTE_HOME", home.as_str())];

    // A fresh override home has no config yet.
    let status = run_in(&db.dir, &["setup", "--status"], home_env);
    assert!(status.status.success());
    let text = stdout_text(&status);
    assert!(text.contains("memo-cli.yml"), "stdout: {text}");
    assert!(text.contains("missing"), "stdout: {text}");

    // Non-interactive: the site variable decides the upgrade source, no prompt.
    let write = run_in(
        &db.dir,
        &["setup"],
        &[
            ("ARIA_COMPUTE_HOME", home.as_str()),
            ("ARIA_MEMO_SITE", "cn"),
        ],
    );
    assert!(
        write.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&write.stderr)
    );
    assert!(stdout_text(&write).contains("wrote"));
    let config_path = db.dir.join("memo-cli.yml");
    assert!(config_path.exists());
    assert!(fs::read_to_string(&config_path)
        .unwrap()
        .contains("gitee.com"));

    let status = run_in(&db.dir, &["setup", "--status"], home_env);
    assert!(stdout_text(&status).contains("upgrade_url: https://gitee.com/ariacompute"));

    // `--clear` removes the file; a second run is a no-op, not an error.
    let clear = run_in(&db.dir, &["setup", "--clear"], home_env);
    assert!(clear.status.success());
    assert!(stdout_text(&clear).contains("cleared"));
    assert!(!config_path.exists());
    let again = run_in(&db.dir, &["setup", "--clear"], home_env);
    assert!(again.status.success());
    assert!(stdout_text(&again).contains("nothing to clear"));
}

#[test]
fn relation_and_graph_flow_across_processes() {
    let db = TempDb::new("graph");
    let a = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "rust systems programming",
        ])
        .trim()
        .to_string();
    let b = db
        .ok(&[
            "add",
            "--type",
            "working",
            "--content",
            "rust error handling",
        ])
        .trim()
        .to_string();

    let related = db.ok(&[
        "relate",
        "--from",
        &a,
        "--to",
        &b,
        "--kind",
        "semantic",
        "--score",
        "0.9",
        "--provenance",
        "e2e",
    ]);
    assert!(related.contains("semantic"), "stdout: {related}");

    let rels = db.json(&["relations", "--from", &a, "--kind", "semantic", "--json"]);
    assert_eq!(rels[0]["from_id"], a);
    assert_eq!(rels[0]["to_id"], b);
    assert_eq!(rels[0]["provenance"], "e2e");

    let graph = db.json(&[
        "graph",
        "--text",
        "rust programming",
        "--views",
        "semantic",
        "--budget",
        "20",
        "--max-hops",
        "3",
        "--top-k",
        "10",
        "--json",
    ]);
    assert!(graph["items"].is_array());
    assert!(!graph["items"].as_array().unwrap().is_empty());
    assert!(graph["trace"]["views"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v == "semantic"));
    assert!(graph["trace"]["stop_reason"].is_string());

    let graph_search = db.json(&["search", "--text", "rust", "--graph", "--json"]);
    assert!(graph_search["items"].is_array());
    assert!(graph_search["trace"].is_object());
}

#[test]
fn invalid_input_exit_codes_across_processes() {
    let db = TempDb::new("invalid");
    for args in [
        vec!["add", "--type", "bogus", "--content", "x"],
        vec!["list", "--type", "bogus", "--json"],
        vec!["graph", "--text", "x", "--views", "bogus", "--json"],
        vec!["relate", "--from", "a", "--to", "b", "--kind", "bogus"],
    ] {
        let out = db.run(&args);
        assert_eq!(out.status.code(), Some(1), "args={args:?}");
        assert!(
            String::from_utf8_lossy(&out.stderr).contains("error:"),
            "args={args:?}"
        );
    }
}
