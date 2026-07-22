# AGENTS.md — memory（端侧长期记忆存储）

工程上下文入口，渐进式披露：先看概述/架构/目录，动手时再看规范/命令/进行中/注意。

## 概述
Rust 端侧（边缘/移动）长期记忆存储，为 LLM Agent 提供 local-first 记忆层。参考 rqlite/turso（嵌入式持久化）与 MemOS/mem0/MemPalace（记忆管理）。M1 交付：三层记忆模型、嵌入式 SQLite 持久化、本地轻量嵌入、语义+关键词混合检索、巩固/去重/遗忘，及可选 CLI。零网络依赖、纯 Rust（不引入重型 ML 框架）。

## 架构（分层 + trait 解耦）
core(模型/错误/trait) → storage(SQLite 持久化) / embed(本地嵌入) → memory(编排) → cli(入口)。
依赖方向单向：memory 依赖 core+storage+embed；storage/embed 仅依赖 core。

## 目录
- crates/core：Memory 模型、MemoryError、MemoryStore/Embedder/StorageBackend trait
- crates/storage：rusqlite 后端（建表/迁移/索引/CRUD/批量写入）+ 复制后端占位
- crates/embed：ngram+哈希/TF-IDF 向量 embedder + 余弦相似度
- crates/memory：manager(增删改查/检索/巩固/去重/遗忘) + lifecycle(分层/衰减/遗忘)
- crates/cli：add/get/search/list/forget/serve
- 根：AGENTS.md / requirements.md / task.md / README.md

## 开发规范
- 统一 `MemoryError`（thiserror），禁止静默失败，禁止 `.unwrap()` 吞错。
- 新增功能同步写单测，核心逻辑必须覆盖正常 + 异常路径；Bug 修复须含可复现用例。
- 平台专属代码用 `#[cfg(...)]` 门控，主构建零平台依赖。
- 核心逻辑纯 Rust；不引入重型 ML 框架；embedder 走本地实现，预留模型接口。

## 常用命令
- `cargo test`：运行全部单测（主目标 x86_64）
- `cargo test -p memory-core`：单 crate 测试
- `cargo build`：构建主目标
- `cargo clippy --all-targets`：静态检查
- `cargo run -p memory-cli -- --help`：查看 CLI

## 进行中需求（M1）
见 task.md。M1 待落地：三层记忆模型、SQLite 持久化、本地嵌入、混合检索、巩固/去重/遗忘、CLI。验收基线：cargo test 全绿、clippy 无告警、交叉编译（aarch64-android/wasm32）通过。

## 注意事项
- 黄金路径：add → embed → 持久化 → search(语义+关键词) → retrieve 端到端跑通且有单测。
- 异常路径（重复 id、缺失、空内容、空嵌入、非法参数、损坏 DB）须有单测覆盖。
- 复制/分布式后端仅做抽象，列为后续里程碑（rqlite 灵感）。
- requirements.md 须经人工逐项审核后方可据其生成 task.md。
