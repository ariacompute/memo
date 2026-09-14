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
        } => {
            let path = std::env::temp_dir().join(format!(
                "aria-memo-bench-{}.db",
                std::process::id()
            ));
            let _ = std::fs::remove_file(&path);
            let manager = build_manager(path.to_str().unwrap_or(":memory:"));
            println!("{}", commands::bench(&manager, size, top_k, warmup)?);
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
                Command::Search { text, top_k, json } => {
                    println!("{}", commands::search(&manager, &text, top_k, json)?);
                }
                Command::Recall { text, top_k, json } => {
                    println!("{}", commands::recall(&manager, &text, top_k, json)?);
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
