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

    // Non-positive elapsed must fail loudly (exit 1 + `error:` on stderr).
    let out = db.run(&["age", "--elapsed-sec", "0"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("error:"));
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
