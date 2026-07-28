//! aria-memory: 端侧长期记忆存储命令行入口。
mod commands;

use clap::{Parser, Subcommand};
use memory::MemoryManager;
use memory_core::{Embedder, Result};
use memory_embed::LocalEmbedder;
use memory_storage::SqliteStore;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "memory", about = "端侧长期记忆存储 CLI")]
struct Cli {
    /// 数据库路径，默认 ./memory.db
    #[arg(long, default_value = "memory.db")]
    db: String,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// 新增一条记忆
    Add {
        #[arg(long, default_value = "working")]
        r#type: String,
        #[arg(long)]
        content: String,
        #[arg(long, default_value_t = 0.5)]
        importance: f32,
    },
    /// 按 id 获取
    Get {
        #[arg(long)]
        id: String,
    },
    /// 混合检索
    Search {
        #[arg(long)]
        text: String,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
    },
    /// 列出记忆
    List {
        #[arg(long)]
        r#type: Option<String>,
    },
    /// 遗忘记忆
    Forget {
        #[arg(long)]
        id: String,
    },
}

fn build_manager(db: &str) -> MemoryManager {
    let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
    let store = SqliteStore::open(db).expect("failed to open store");
    MemoryManager::new(embedder, Arc::new(store))
}

fn run(cli: Cli) -> Result<()> {
    let manager = build_manager(&cli.db);
    match cli.command {
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
        Command::Search { text, top_k } => {
            println!("{}", commands::search(&manager, &text, top_k)?);
        }
        Command::List { r#type } => {
            println!("{}", commands::list(&manager, r#type.as_deref())?);
        }
        Command::Forget { id } => {
            println!("{}", commands::forget(&manager, &id)?);
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
