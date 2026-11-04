//! aria-memo: on-device long-term memory storage command-line entry point.
mod commands;
mod config;
mod setup;
#[cfg(test)]
mod test_support;
mod upgrade;

use crate::setup::SetupArgs;

use clap::{ArgAction, Parser, Subcommand};
use memo::MemoManager;
use memo_core::{Embedder, Result};
use memo_embed::LocalEmbedder;
use memo_storage::SqliteStore;
use std::cell::OnceCell;
use std::ffi::OsString;
use std::sync::Arc;

const MEMO_VERSION: &str = env!("ARIA_MEMO_VERSION");

#[derive(Parser)]
#[command(
    name = "memo",
    about = "On-device long-term memory storage CLI",
    version = MEMO_VERSION,
    disable_version_flag = true
)]
struct Cli {
    /// Print version
    #[arg(short = 'v', long = "version", action = ArgAction::Version)]
    _version: (),
    /// Database path, defaults to ./memo.db
    #[arg(long, default_value = "memo.db")]
    db: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Add a memory
    Add {
        #[arg(long, default_value = "working")]
        r#type: String,
        #[arg(long)]
        content: String,
        #[arg(long, default_value_t = 0.5)]
        importance: f32,
    },
    /// Fetch by id
    Get {
        #[arg(long)]
        id: String,
    },
    /// Hybrid search
    Search {
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Machine-readable JSON output (id/score/content); default is score\tcontent
        #[arg(long, default_value_t = false)]
        json: bool,
        /// Graph-aware retrieval: hybrid-seed a bounded traversal (all four views)
        #[arg(long, default_value_t = false)]
        graph: bool,
    },
    /// Pure vector (semantic) recall — cosine similarity only
    Recall {
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Machine-readable JSON output (id/score/content); default is score\tcontent
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Batch hybrid search over multiple queries (repeat `--text` for each query)
    SearchBatch {
        #[arg(long)]
        text: Vec<String>,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        /// Machine-readable JSON output (per-query id/score/content); default is score\tcontent
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// List memories
    List {
        #[arg(long)]
        r#type: Option<String>,
        /// Machine-readable JSON output (id/type/content/importance/version/metadata); default is human-readable
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Update a memory by id
    Update {
        #[arg(long)]
        id: String,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        r#type: Option<String>,
        #[arg(long)]
        importance: Option<f32>,
    },
    /// Forget a memory
    Forget {
        #[arg(long)]
        id: String,
    },
    /// Write→connect: infer four-view relations for an existing memory by id
    Connect {
        #[arg(long)]
        id: String,
    },
    /// Manually add a relation edge between two memories
    Relate {
        #[arg(long)]
        from: String,
        #[arg(long)]
        to: String,
        #[arg(long)]
        kind: String,
        #[arg(long, default_value_t = 0.8)]
        score: f32,
        #[arg(long)]
        provenance: Option<String>,
    },
    /// List relations (optionally filtered by from/to/kind)
    Relations {
        #[arg(long)]
        from: Option<String>,
        #[arg(long)]
        to: Option<String>,
        #[arg(long)]
        kind: Option<String>,
        #[arg(long, default_value_t = 50)]
        top_k: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// Graph-aware retrieval: hybrid-seed a bounded multi-relational traversal
    Graph {
        #[arg(long)]
        text: String,
        /// Comma-separated views (semantic/temporal/causal/entity); empty = all
        #[arg(long, default_value = "")]
        views: String,
        #[arg(long, default_value_t = 60)]
        budget: usize,
        #[arg(long, default_value_t = 3)]
        max_hops: usize,
        #[arg(long, default_value_t = 20)]
        top_k: usize,
        #[arg(long, default_value_t = false)]
        json: bool,
    },
    /// In-process micro-benchmark (JSON), parsed by benches/
    Bench {
        /// Number of writes and searches (corpus size)
        #[arg(long, default_value_t = 1000)]
        size: usize,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long, default_value_t = 10)]
        warmup: usize,
        /// Kept for compatibility; output is always JSON
        #[arg(long, default_value_t = true)]
        json: bool,
        /// Also measure batch retrieval throughput (single lock over all queries)
        #[arg(long, default_value_t = false)]
        batch: bool,
        /// Enable WAL journal mode and report the `add_wal` write-tail segment
        #[arg(long, default_value_t = false)]
        wal: bool,
        /// Measure the `add_batch_embed` segment (batch embed + transactional batch write)
        #[arg(long, default_value_t = false)]
        batch_embed: bool,
        /// Measure the `add_bulk` segment (per-item embed + transaction merge)
        #[arg(long, default_value_t = false)]
        bulk: bool,
        /// Skip the `add_baseline` (per-item embed + per-item txn) segment. Use for
        /// large corpora where the naive path would exceed the CLI timeout.
        #[arg(long, default_value_t = false)]
        no_baseline: bool,
        /// Override repetitions for the batched write-tail segments (default: warmup+1).
        #[arg(long)]
        reps: Option<usize>,
    },
    /// Write CLI config (~/.ariacompute/memo-cli.yml)
    Setup(Box<SetupArgs>),
    /// Replace this CLI from Releases
    Upgrade {
        /// Target version (default: latest stable)
        version: Option<String>,
        /// Override the upgrade_url from config
        #[arg(long = "url")]
        url: Option<String>,
    },
    /// Print version
    Version,
}

fn build_manager(db: &str) -> MemoManager {
    let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
    let store = SqliteStore::open(db).expect("failed to open store");
    MemoManager::new(embedder, Arc::new(store))
}

/// Execute one parsed command. The store is opened lazily so commands that do
/// not touch memories (`version` / `setup` / `upgrade`) never create a database.
fn run(cli: Cli) -> Result<()> {
    let Cli { db, command, .. } = cli;
    let manager: OnceCell<MemoManager> = OnceCell::new();
    let store = || manager.get_or_init(|| build_manager(&db));

    match command {
        Command::Setup(args) => setup::run(*args)?,
        Command::Upgrade { version, url } => {
            upgrade::run(version.as_deref(), MEMO_VERSION, url.as_deref())?
        }
        Command::Version => println!("aria-memo {MEMO_VERSION}"),
        Command::Bench {
            size,
            top_k,
            warmup,
            json: _,
            batch,
            wal,
            batch_embed,
            bulk,
            no_baseline,
            reps,
        } => {
            let path =
                std::env::temp_dir().join(format!("aria-memo-bench-{}.db", std::process::id()));
            let _ = std::fs::remove_file(&path);
            let bench_manager = build_manager(path.to_str().unwrap_or(":memory:"));
            let cfg = commands::BenchConfig {
                size,
                top_k,
                warmup,
                search_batch: batch,
                wal,
                batch_embed,
                bulk,
                baseline: !no_baseline,
                reps,
            };
            println!("{}", commands::bench(&bench_manager, &cfg)?);
            let _ = std::fs::remove_file(&path);
        }
        Command::Add {
            r#type,
            content,
            importance,
        } => {
            let id = commands::add(store(), &r#type, &content, importance)?;
            println!("{id}");
        }
        Command::Get { id } => {
            println!("{}", commands::get(store(), &id)?);
        }
        Command::Search {
            text,
            top_k,
            json,
            graph,
        } => {
            if graph {
                println!(
                    "{}",
                    commands::graph_retrieve(store(), &text, "", 60, 3, top_k, json)?
                );
            } else {
                println!("{}", commands::search(store(), &text, top_k, json)?);
            }
        }
        Command::Recall { text, top_k, json } => {
            println!("{}", commands::recall(store(), &text, top_k, json)?);
        }
        Command::SearchBatch { text, top_k, json } => {
            println!("{}", commands::search_batch(store(), &text, top_k, json)?);
        }
        Command::List { r#type, json } => {
            println!("{}", commands::list(store(), r#type.as_deref(), json)?);
        }
        Command::Update {
            id,
            content,
            r#type,
            importance,
        } => {
            commands::update(
                store(),
                &id,
                content.as_deref(),
                r#type.as_deref(),
                importance,
            )?;
            println!("updated {id}");
        }
        Command::Forget { id } => {
            println!("{}", commands::forget(store(), &id)?);
        }
        Command::Connect { id } => {
            println!("{}", commands::connect(store(), &id)?);
        }
        Command::Relate {
            from,
            to,
            kind,
            score,
            provenance,
        } => {
            println!(
                "{}",
                commands::relate(store(), &from, &to, &kind, score, provenance.as_deref())?
            );
        }
        Command::Relations {
            from,
            to,
            kind,
            top_k,
            json,
        } => {
            let filters = (from.as_deref(), to.as_deref(), kind.as_deref());
            let out = commands::relations(store(), filters.0, filters.1, filters.2, top_k, json)?;
            println!("{out}");
        }
        Command::Graph {
            text,
            views,
            budget,
            max_hops,
            top_k,
            json,
        } => {
            println!(
                "{}",
                commands::graph_retrieve(store(), &text, &views, budget, max_hops, top_k, json)?
            );
        }
    }
    Ok(())
}

/// Parse `args` and run the command, returning the process exit code
/// (`0` success, `1` runtime error, `2` usage error; help/version print and
/// exit `0` like `clap`'s `parse()`).
pub(crate) fn entry<I, T>(args: I) -> i32
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    match Cli::try_parse_from(args) {
        Ok(cli) => match run(cli) {
            Ok(()) => 0,
            Err(e) => {
                eprintln!("error: {e}");
                1
            }
        },
        Err(e) => {
            let code = if e.use_stderr() { 2 } else { 0 };
            let _ = e.print();
            code
        }
    }
}

fn main() {
    std::process::exit(entry(std::env::args_os()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::{env_lock, temp_dir};
    use std::path::PathBuf;

    fn cli(db: &str, command: Command) -> Cli {
        Cli {
            _version: (),
            db: db.to_string(),
            command,
        }
    }

    /// A file-backed database shared across sequential `run` calls.
    struct Db {
        dir: PathBuf,
        path: String,
    }

    impl Db {
        fn new(tag: &str) -> Self {
            let dir = temp_dir(tag);
            let path = dir.join("memo.db").to_str().unwrap().to_string();
            Self { dir, path }
        }
    }

    impl Drop for Db {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.dir);
        }
    }

    fn ids(db: &str) -> Vec<String> {
        build_manager(db)
            .list(None)
            .unwrap()
            .into_iter()
            .map(|m| m.id)
            .collect()
    }

    #[test]
    fn version_command_prints_and_succeeds() {
        run(cli(":memory:", Command::Version)).unwrap();
        assert_eq!(entry(["aria-memo", "version"]), 0);
    }

    #[test]
    fn memo_commands_run_through_the_cli() {
        let db = Db::new("main-memo");
        let d = db.path.as_str();

        run(cli(
            d,
            Command::Add {
                r#type: "working".into(),
                content: "user likes rust systems programming".into(),
                importance: 0.7,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Add {
                r#type: "short_term".into(),
                content: "user is booking a flight to oslo".into(),
                importance: 0.4,
            },
        ))
        .unwrap();
        let all = ids(d);
        assert_eq!(all.len(), 2);
        let first = all[0].clone();
        let second = all[1].clone();

        run(cli(d, Command::Get { id: first.clone() })).unwrap();
        run(cli(
            d,
            Command::List {
                r#type: None,
                json: true,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::List {
                r#type: Some("working".into()),
                json: false,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Search {
                text: "rust".into(),
                top_k: 5,
                json: true,
                graph: false,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Search {
                text: "rust".into(),
                top_k: 5,
                json: false,
                graph: true,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::SearchBatch {
                text: vec!["rust".into(), "oslo".into()],
                top_k: 5,
                json: true,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Recall {
                text: "rust".into(),
                top_k: 5,
                json: false,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Update {
                id: first.clone(),
                content: Some("user loves rust systems programming".into()),
                r#type: Some("long_term:semantic".into()),
                importance: Some(0.9),
            },
        ))
        .unwrap();
        run(cli(d, Command::Connect { id: first.clone() })).unwrap();
        run(cli(
            d,
            Command::Relate {
                from: first.clone(),
                to: second.clone(),
                kind: "causal".into(),
                score: 0.7,
                provenance: Some("manual".into()),
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Relations {
                from: Some(first.clone()),
                to: None,
                kind: Some("causal".into()),
                top_k: 10,
                json: true,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Relations {
                from: None,
                to: None,
                kind: None,
                top_k: 10,
                json: false,
            },
        ))
        .unwrap();
        run(cli(
            d,
            Command::Graph {
                text: "rust".into(),
                views: "semantic,causal".into(),
                budget: 20,
                max_hops: 2,
                top_k: 5,
                json: true,
            },
        ))
        .unwrap();
        run(cli(d, Command::Forget { id: first })).unwrap();
        assert_eq!(ids(d).len(), 1);
    }

    #[test]
    fn memo_commands_surface_errors() {
        let db = Db::new("main-errors");
        let d = db.path.as_str();
        // `get` on an unknown id prints "not found" but is not an error.
        run(cli(
            d,
            Command::Get {
                id: "missing".into(),
            },
        ))
        .unwrap();
        assert!(run(cli(
            d,
            Command::Update {
                id: "missing".into(),
                content: Some("x".into()),
                r#type: None,
                importance: None,
            }
        ))
        .is_err());
        assert!(run(cli(
            d,
            Command::Relate {
                from: "a".into(),
                to: "b".into(),
                kind: "bogus".into(),
                score: 0.5,
                provenance: None,
            }
        ))
        .is_err());
    }

    #[test]
    fn bench_command_runs_with_a_tiny_corpus() {
        run(cli(
            ":memory:",
            Command::Bench {
                size: 2,
                top_k: 2,
                warmup: 0,
                json: true,
                batch: true,
                wal: true,
                batch_embed: true,
                bulk: true,
                no_baseline: false,
                reps: Some(1),
            },
        ))
        .unwrap();
    }

    #[test]
    fn setup_command_runs_against_a_temp_home() {
        let _g = env_lock();
        let home = temp_dir("main-setup");
        std::env::set_var("ARIA_COMPUTE_HOME", &home);
        std::env::remove_var("ARIA_MEMO_SITE");
        run(cli(
            ":memory:",
            Command::Setup(Box::new(SetupArgs {
                status: true,
                clear: false,
            })),
        ))
        .unwrap();
        run(cli(
            ":memory:",
            Command::Setup(Box::new(SetupArgs {
                status: false,
                clear: true,
            })),
        ))
        .unwrap();
        std::env::remove_var("ARIA_COMPUTE_HOME");
        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn upgrade_command_surfaces_errors() {
        assert!(run(cli(
            ":memory:",
            Command::Upgrade {
                version: None,
                url: Some("https://example.com/ariacompute".into()),
            }
        ))
        .is_err());
    }

    #[test]
    fn entry_maps_usage_and_runtime_errors_to_exit_codes() {
        // Unknown flag / missing required argument -> usage error.
        assert_eq!(entry(["aria-memo", "--nope"]), 2);
        assert_eq!(entry(["aria-memo", "add"]), 2);
        // Help and version are informational, not errors.
        assert_eq!(entry(["aria-memo", "--help"]), 0);
        assert_eq!(entry(["aria-memo", "--version"]), 0);
        // Runtime failure -> 1 (updating an unknown id is NotFound).
        assert_eq!(
            entry([
                "aria-memo",
                "--db",
                ":memory:",
                "update",
                "--id",
                "missing",
                "--content",
                "x"
            ]),
            1
        );
    }
}
