//! aria-memo: on-device long-term memory storage command-line entry point.
mod commands;
mod config;
mod setup;
mod upgrade;

use crate::setup::SetupArgs;

use clap::{ArgAction, Parser, Subcommand};
use memo::MemoManager;
use memo_core::{Embedder, Result};
use memo_embed::LocalEmbedder;
use memo_storage::SqliteStore;
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

fn run(cli: Cli) -> Result<()> {
    match cli.command {
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
        } => {
            let path = std::env::temp_dir().join(format!(
                "aria-memo-bench-{}.db",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            let manager = build_manager(path.to_str().unwrap_or(":memory:"));
            let cfg = commands::BenchConfig {
                size,
                top_k,
                warmup,
                search_batch: batch,
                wal,
                batch_embed,
                bulk,
            };
            println!("{}", commands::bench(&manager, &cfg)?);
            let _ = std::fs::remove_file(&path);
        }
        other => {
            let manager = build_manager(&cli.db);
            match other {
                Command::Add {
                    r#type,
                    content,
                    importance,
                } => {
                    let id = commands::add(&manager, &r#type, &content, importance)?;
                    println!("{id}");
                }
                Command::Get { id } => {
                    println!("{}", commands::get(&manager, &id)?);
                }
                Command::Search { text, top_k, json, graph } => {
                    if graph {
                        println!(
                            "{}",
                            commands::graph_retrieve(&manager, &text, "", 60, 3, top_k, json)?
                        );
                    } else {
                        println!("{}", commands::search(&manager, &text, top_k, json)?);
                    }
                }
                Command::Recall { text, top_k, json } => {
                    println!("{}", commands::recall(&manager, &text, top_k, json)?);
                }
                Command::SearchBatch { text, top_k, json } => {
                    println!("{}", commands::search_batch(&manager, &text, top_k, json)?);
                }
                Command::List { r#type, json } => {
                    println!("{}", commands::list(&manager, r#type.as_deref(), json)?);
                }
                Command::Update {
                    id,
                    content,
                    r#type,
                    importance,
                } => {
                    commands::update(&manager, &id, content.as_deref(), r#type.as_deref(), importance)?;
                    println!("updated {id}");
                }
                Command::Forget { id } => {
                    println!("{}", commands::forget(&manager, &id)?);
                }
                Command::Connect { id } => {
                    println!("{}", commands::connect(&manager, &id)?);
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
                        commands::relate(&manager, &from, &to, &kind, score, provenance.as_deref())?
                    );
                }
                Command::Relations {
                    from,
                    to,
                    kind,
                    top_k,
                    json,
                } => {
                    println!(
                        "{}",
                        commands::relations(
                            &manager,
                            from.as_deref(),
                            to.as_deref(),
                            kind.as_deref(),
                            top_k,
                            json
                        )?
                    );
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
                        commands::graph_retrieve(&manager, &text, &views, budget, max_hops, top_k, json)?
                    );
                }
                Command::Bench { .. } => unreachable!(),
                // Already handled above; silence exhaustiveness.
                Command::Setup(_) | Command::Upgrade { .. } | Command::Version => unreachable!(),
            }
        }
    }
    Ok(())
}

fn main() {
    let cli = Cli::parse();
    if let Err(e) = run(cli) {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
