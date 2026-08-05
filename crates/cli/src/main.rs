//! aria-memo: 端侧长期记忆存储命令行入口。
mod commands;

use clap::{Parser, Subcommand};
use memo::MemoManager;
use memo_core::{Embedder, Result};
use memo_embed::LocalEmbedder;
use memo_storage::SqliteStore;
use std::sync::Arc;

#[derive(Parser)]
#[command(name = "memo", about = "端侧长期记忆存储 CLI")]
struct Cli {
    /// 数据库路径，默认 ./memo.db
    #[arg(long, default_value = "memo.db")]
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
    /// 进程内微基准（JSON），供 benches/ 解析
    Bench {
        /// 写入与检索次数（库规模）
        #[arg(long, default_value_t = 1000)]
        size: usize,
        #[arg(long, default_value_t = 5)]
        top_k: usize,
        #[arg(long, default_value_t = 10)]
        warmup: usize,
        /// 保留兼容；输出始终为 JSON
        #[arg(long, default_value_t = true)]
        json: bool,
    },
}

fn build_manager(db: &str) -> MemoManager {
    let embedder: Arc<dyn Embedder> = Arc::new(LocalEmbedder::new(64));
    let store = SqliteStore::open(db).expect("failed to open store");
    MemoManager::new(embedder, Arc::new(store))
}

fn run(cli: Cli) -> Result<()> {
    match cli.command {
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
                Command::Search { text, top_k } => {
                    println!("{}", commands::search(&manager, &text, top_k)?);
                }
                Command::List { r#type } => {
                    println!("{}", commands::list(&manager, r#type.as_deref())?);
                }
                Command::Forget { id } => {
                    println!("{}", commands::forget(&manager, &id)?);
                }
                Command::Bench { .. } => unreachable!(),
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
