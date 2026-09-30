# AGENTS.md — memo（端侧长期记忆存储）

工程上下文入口，渐进式披露：先看概述/架构/目录，动手时再看规范/命令/进行中/注意。

## 概述
Rust 端侧（边缘/移动）长期记忆存储，为 LLM Agent 提供 local-first 记忆层。参考 rqlite/turso（嵌入式持久化）与 MemOS/mem0/MemPalace（记忆管理）。M1：三层记忆、SQLite、本地嵌入、混合检索 search + 纯向量召回 recall、巩固/去重/遗忘、CLI。M2：与 mem0/MemOS/MemPalace/Zep/Letta 的功能矩阵 + Track A/B 评测（`benches/` Python）。M3：Track B 四基准真实评测管线（locomo_refined/halumem/longmemeval/personamem）。M4：多关系记忆平面（Jev-Mem 启发，semantic/temporal/causal/entity 四视图有向边，local-first 离线默认，可插拔 LLM scorer）。零网络依赖、纯 Rust（不引入重型 ML 框架）。

## 架构（分层 + trait 解耦）
core(模型/错误/trait) → storage(SQLite 持久化) / embed(本地嵌入) → memo(编排) → cli(入口)。
依赖方向单向：memo 依赖 core+storage+embed；storage/embed 仅依赖 core。

## 目录
- crates/core：Memo 模型、MemoError、MemoStore/Embedder/StorageBackend trait
- crates/storage：rusqlite 后端（建表/迁移/索引/CRUD/批量写入）+ 复制后端占位
- crates/embed：ngram+哈希/TF-IDF 向量 embedder + 余弦相似度
- crates/memo：manager(增删改查/检索(search 混合 + recall 纯向量)/巩固/去重/遗忘) + lifecycle(分层/衰减/遗忘) + MemoryController + RelationScorer（write→connect 推断四视图边 / retrieve→assess→expand 有界图遍历，附带可检视 RetrieveTrace；默认 LocalRelationScorer 离线）
- crates/cli：`add/get/search/recall/list/update/forget/bench` + 自管理 `setup`(`--status`/`--clear`，交互选择 GitHub/Gitee 升级源) 与 `upgrade [version]`(`--url` 可选覆盖) ；`--version`/`-v` 打印版本（list/search 支持 `--json` 机器可读输出）。关系平面子命令：`connect`(按 id 推断四视图边) / `relate`(手动建边) / `relations`(列边 by from/to/kind) / `graph`(有界多关系遍历，views/budget/max_hops/top_k，--json 输出含 RetrieveTrace)；`search --graph` 等价开启图感知检索。
- benches/：Python 评测编排（Track A 微基准+合成检索；Track B 四基准 locomo_refined/halumem/longmemeval/personamem）
- docs/：compare.md 功能矩阵、bench_results.md 结果说明
- 根：AGENTS.md / requirements.md / task.md / README.md

## 开发规范
- 统一 `MemoError`（thiserror），禁止静默失败，禁止 `.unwrap()` 吞错。
- 新增功能同步写单测，核心逻辑必须覆盖正常 + 异常路径；Bug 修复须含可复现用例。
- 平台专属代码用 `#[cfg(...)]` 门控，主构建零平台依赖。
- 核心逻辑纯 Rust；不引入重型 ML 框架；embedder 走本地实现，预留模型接口。
- 业界评测编排放 `benches/`（Python），禁止新增 `crates/bench`。

## 常用命令
- `cargo test` / `cargo test -p memo-core` / `cargo build` / `cargo clippy --all-targets`
- `cargo run -p aria-memo -- --help` / `cargo run -p aria-memo -- bench --size 100 --json` / `cargo run -p aria-memo -- recall --text "rust systems programming" --top-k 5`
- `python benches/run.py --track a` / `python benches/run.py --track b --dry-run`

## 进行中需求
- M1：见 task.md（已落地）。
- M2：功能对比 + Track A/B 评测；验收见 requirements.md §6.7。
- M3：Track B 四基准真实评测管线（locomo_refined/halumem/longmemeval/personamem）；CLI 增 `update` 与 `list/search --json` 供 HaluMem 操作级评测；judge 可选（缺凭据 skip 不伪造分数）。详见 task.md M3。
- M4：多关系记忆平面（Jev-Mem 启发）。已落地（commit `13ab4ec`）：`memo-core` 增加 `RelationKind`/`Relation`/`GraphRetrieveQuery`/`RetrieveTrace`/`GraphRetrieveResult` 与 `graph_bfs`；`MemoStore` 扩展 `add_relation`/`get_relations`/`delete_relations`/`expand`；`storage/sqlite` 新增独立 `relations` 表 + 四视图 CRUD 与有界 BFS `expand`；`memo` 新增 `MemoryController` + `RelationScorer`（默认 `LocalRelationScorer` 离线）；CLI 新增 `connect`/`relate`/`relations`/`graph` 与 `search --graph`。零网络依赖，可插拔 LLM scorer。详见 requirements.md §1.3 / §2.5–§2.8 / task.md M4。

## 注意事项
- 黄金路径：add → embed → 持久化 → search/recall → retrieve 端到端单测。
- 异常路径（重复 id、缺失、空内容、空嵌入、非法参数（importance 越界 / top_k=0 / query 文本空）、损坏/非法 metadata DB、recall/search 拒绝非法 query）须有单测。
- 复制/分布式后端仅抽象，后续里程碑。
- 自管理：`aria-memo setup` 持久化 CLI 配置到 `~/.ariacompute/memo-cli.yml`（`upgrade_url` = Releases 组织根，默认 `https://github.com/ariacompute`；交互时可选择 GitHub `https://github.com/ariacompute` 或 Gitee `https://gitee.com/ariacompute`；home 可由 `ARIA_COMPUTE_HOME` 覆盖，`cn` 站点经 `ARIA_MEMO_SITE=cn` 默认 Gitee，`--clear` 删除配置）。`aria-memo upgrade [version]` 从 GitHub/Gitee Releases 下载当前平台二进制并原地替换；`upgrade_url` 优先级 `--url` > `memo-cli.yml`(优先读取) > `ARIA_MEMO_UPGRADE_URL` > 内置默认（`https://github.com/ariacompute`）。
- Release 资产命名约定：`aria-memo_{ver}_{os}.tar.gz`（`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`，Windows 为 `.zip`）；CI 须按此命名，升级链路才闭环（参考 router `bin/src/upgrade.rs`）。
- Track B 离线指标（F1/BLEU/多选/Recall@k）零网络可出；judge 指标依赖 OpenAI 兼容 LLM（BENCH_LLM_API_KEY），缺则 skip 并写 reason，不伪造分数。
- `data/fixtures/<bench>/` 为内置合成样例（仅供单测/冒烟），报告标注 `dataset_source: fixture`，不可与正式基准分数直接对比。
- requirements.md 须经人工逐项审核后方可据其生成 task.md。
- 关系平面（M4）：`relations` 存于独立表，绝不改动 `Memo`；边为四视图有向边（semantic/temporal/causal/entity），默认 scorer 离线（semantic=cosine / temporal=时间序 / entity=token Jaccard / causal=时间邻+重叠），阈值/候选数/每写最大边数由 `RelationConfig` 控制；`connect` 须待记忆已持久化（write→connect 顺序），返回建边数；自环、越界 score（非 [0,1]）、空 provenance 经 `Relation::validate` 拒绝；`delete_relations` 至少需一个过滤条件否则 `InvalidParam`；`graph_bfs` 按 0.9/hop 衰减、按 budget 封顶、去环。
