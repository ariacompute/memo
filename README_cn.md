# aria memory

> 文档语言：[中文](./README_cn.md) ｜ [English](./README.md)

Rust 实现的边缘/移动端本地优先（local-first）长期记忆存储，为 LLM Agent 提供记忆的增删改查、语义 + 关键词混合检索、巩固、去重与遗忘。零网络依赖、纯 Rust（不引入重型 ML 框架）。

参考：rqlite / turso（嵌入式持久化）、MemOS / mem0 / MemPalace（记忆管理）。

## 分层架构

分层 cargo workspace（trait 解耦）：

```
cli(aria-memory) → memory(编排) → storage(SQLite) / embed(本地嵌入) → core(模型/错误/trait)
```

## 快速开始

```bash
cargo build
cargo test
cargo run -p aria-memory -- add --type working --content "用户喜欢 Rust" --importance 0.8
cargo run -p aria-memory -- search --text "Rust" --top-k 5
```

## 目录

- `crates/core` — 数据模型、统一错误 `MemoryError`、trait
- `crates/embed` — 本地轻量 embedder（ngram + 哈希/TF-IDF 向量）+ 余弦相似度
- `crates/storage` — rusqlite 嵌入式持久化后端
- `crates/memory` — 记忆管理编排与生命周期
- `crates/cli` — 命令行入口
