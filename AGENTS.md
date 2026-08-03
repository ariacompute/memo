# AGENTS.md — memory（端侧长期记忆存储）

工程上下文入口，渐进式披露：先看概述/架构/目录，动手时再看规范/命令/进行中/注意。

## 概述
Rust 端侧（边缘/移动）长期记忆存储，为 LLM Agent 提供 local-first 记忆层。参考 rqlite/turso（嵌入式持久化）与 MemOS/mem0/MemPalace（记忆管理）。M1：三层记忆、SQLite、本地嵌入、混合检索、巩固/去重/遗忘、CLI。M2：与 mem0/MemOS/MemPalace/Zep/Letta 的功能矩阵 + Track A/B 评测（`benches/` Python）。零网络依赖、纯 Rust（不引入重型 ML 框架）。

## 架构（分层 + trait 解耦）
core(模型/错误/trait) → storage(SQLite 持久化) / embed(本地嵌入) → memory(编排) → cli(入口)。
依赖方向单向：memory 依赖 core+storage+embed；storage/embed 仅依赖 core。

## 目录
- crates/core：Memory 模型、MemoryError、MemoryStore/Embedder/StorageBackend trait
- crates/storage：rusqlite 后端（建表/迁移/索引/CRUD/批量写入）+ 复制后端占位
- crates/embed：ngram+哈希/TF-IDF 向量 embedder + 余弦相似度
- crates/memory：manager(增删改查/检索/巩固/去重/遗忘) + lifecycle(分层/衰减/遗忘)
- crates/cli：add/get/search/list/forget/bench/serve
- benches/：Python 评测编排（Track A 微基准+合成检索；Track B LoCoMo/LongMemEval/BEAM）
- docs/：compare.md 功能矩阵、bench_results.md 结果说明
- 根：AGENTS.md / requirements.md / task.md / README.md

## 开发规范
- 统一 `MemoryError`（thiserror），禁止静默失败，禁止 `.unwrap()` 吞错。
- 新增功能同步写单测，核心逻辑必须覆盖正常 + 异常路径；Bug 修复须含可复现用例。
- 平台专属代码用 `#[cfg(...)]` 门控，主构建零平台依赖。
- 核心逻辑纯 Rust；不引入重型 ML 框架；embedder 走本地实现，预留模型接口。
- 业界评测编排放 `benches/`（Python），禁止新增 `crates/bench`。

## 常用命令
- `cargo test` / `cargo test -p memory-core` / `cargo build` / `cargo clippy --all-targets`
- `cargo run -p aria-memory -- --help` / `cargo run -p aria-memory -- bench --size 100 --json`
- `python benches/run.py --track a` / `python benches/run.py --track b --dry-run`

## 进行中需求
- M1：见 task.md（已落地）。
- M2：功能对比 + Track A/B 评测；验收见 requirements.md §6.7。

## 注意事项
- 黄金路径：add → embed → 持久化 → search → retrieve 端到端单测。
- 异常路径（重复 id、缺失、空内容、空嵌入、非法参数、损坏 DB）须有单测。
- 复制/分布式后端仅抽象，后续里程碑。
- Track B 依赖外部 LLM 时须在报告标明模型与非离线；缺密钥 skip 并写原因。
- requirements.md 须经人工逐项审核后方可据其生成 task.md。
