# memo

[English](README.md) | [中文](README_cn.md)

Rust 实现的边缘/移动端本地优先（local-first）长期记忆存储，为 LLM Agent 提供记忆的增删改查、语义 + 关键词混合检索、巩固、去重与遗忘。零网络依赖、纯 Rust（不引入重型 ML 框架）。

参考：rqlite / turso（嵌入式持久化）、MemOS / mem0 / MemPalace（记忆管理）。

## 分层架构

分层 cargo workspace（trait 解耦）：

```
cli(aria-memo) → memo(编排) → storage(SQLite) / embed(本地嵌入) → core(模型/错误/trait)
```

## 快速开始

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "用户喜欢 Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

## 对比评测

对比系统：mem0 / MemOS / MemPalace / Zep / Letta。

- 功能矩阵：[docs/compare.md](./docs/compare.md)
- 评测说明与结果：[docs/bench_results.md](./docs/bench_results.md)
- Python 编排（Track A 存储/检索 + Track B 端到端质量）：[benches/README.md](./benches/README.md)

```bash
cargo run -p aria-memo -- bench --size 1000 --json
pip install -r benches/requirements.txt
python benches/run.py --track a --size 1000
python benches/run.py --track b --dry-run
```

## 目录

- `crates/core` — 数据模型、统一错误 `MemoError`、trait
- `crates/embed` — 本地轻量 embedder（ngram + 哈希/TF-IDF 向量）+ 余弦相似度
- `crates/storage` — rusqlite 嵌入式持久化后端
- `crates/memo` — 记忆管理编排与生命周期
- `crates/cli` — 命令行入口
- `benches/` — 业界对比评测
- `docs/` — 功能矩阵与评测结果说明

## 工程规范

本仓库遵循 Harness Engineering 理念：

- [`AGENTS.md`](AGENTS.md)：Agent 工程上下文入口与目录索引
- [`requirements.md`](requirements.md)：需求规格（功能边界/异常/验收标准，人工审核制）
- [`task.md`](task.md)：实施任务清单
