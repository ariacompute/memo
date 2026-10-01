# task.md — memo 端侧长期记忆存储实施清单

> 由 `requirements.md`（已人工审核通过）生成的分步清单。
> 统一验收基线：`cargo test` 全绿、`cargo clippy --all-targets` 无告警、交叉编译（wasm32）通过；M2 另需 `benches/` Track A/B 可跑。

## M1（已完成）

1. [x] 初始化 cargo workspace 与 5 crate 脚手架（core/storage/embed/memo/cli）+ 基线，`cargo build` 通过
2. [x] 产出 `AGENTS.md`（≤100 行）与 `requirements.md` 供人工审核
3. [x] 人工逐项审核通过 `requirements.md`，生成本 `task.md`
4. [x] 实现 `memo-core`：`MemoError`、数据模型与校验、trait、工具 + 单测
5. [x] 实现 `memo-embed`：`LocalEmbedder`、`cosine` + 单测
6. [x] 实现 `memo-storage`：`SqliteStore` + 单测
7. [x] 实现 `memo`：`MemoManager`、`lifecycle` + 单测
8. [x] 实现 `aria-memo`：add/get/search/list/forget + 单测
9. [x] 验收：`cargo test` 全绿、clippy 无告警、wasm32 编译通过
10. [x] 新增纯向量召回：`memo-core` 增加 `RecallQuery`（余弦-only 打分 + `validate()`）；`MemoManager::recall` 按余弦相似度排序（与混合 `search` 区分）；CLI 新增 `recall` 子命令（`score\tcontent` / `--json`）
11. [x] 全仓单测补强（正常 + 异常路径）：core（`MemoPatch` 空校验、`RecallQuery` 校验、`memo_type` FromStr、`embedding` 序列化/反序列化长度不匹配）、embed（tokenize 单/二元、`hash_dim` 范围与确定性、零模长 cosine、`keyword` 部分/零分）、memo（`update` 仅类型/仅 importance/空 content 与 importance 越界、`dedup` 非法阈值/noop、`consolidate` 缺失 id→NotFound、`search`/`recall` 拒绝非法 query、recall 复用外部 embedding 跳过 embedder）、storage（in-memory 构建、批量 add、损坏/非法 metadata JSON、`backend` kind 往返）、cli（`bench` 拒绝 top_k=0、`get` 缺失/`search` 空）、lifecycle（`decay` 缩减并跳过非法、`prune` 非法 floor/noop）

## M2 — 业界功能/性能评测（A + B）

> 对比系统：mem0 / MemOS / MemPalace / Zep / Letta。编排工程：`benches/`（Python），不采用 `crates/bench`。

10. [x] `requirements.md` §6 增补 Track A/B、矩阵、验收；本清单同步
11. [x] `docs/compare.md` 功能对比矩阵（aria + 五系统）
12. [x] CLI `memo bench --json`（进程内 add/search 微基准，供 Python 解析）
13. [x] `benches/` 脚手架：`common` / `adapters` / `track_a` / `track_b` / `data` / `results` / `run.py`
14. [x] Track A：微基准汇总（1k/10k）+ 合成检索 Recall@k / MRR（aria 必跑；他系统按可用性）
15. [x] Track B：LoCoMo / LongMemEval / BEAM runner 骨架 + dry-run；五系统 adapter 接口齐全
16. [x] 文档：`benches/README.md`、`docs/bench_results.md`、根 README / AGENTS 链到评测

## M3 — Track B 四基准真实评测管线（当前进行中）

> 需求：基于 LoCoMo-Refined / HaluMem / LongMemEval / PersonaMem 进行 memo 评测。
> 已澄清：移除 beam + 旧 locomo；完整管线（loader+检索+离线指标真跑，judge 可选）；下载脚本+fixture；扩展 HaluMem 操作级能力并同步 CLI。

17. [ ] `requirements.md` §1.2b/§2.4/§6.4/§6.5/§6.7 已更新（本步：规格先行，已落）
18. [ ] CLI 增补：`update` 子命令 + `list --json` / `search --json`；保留默认人类可读输出；`commands.rs` 新增正常+异常单测
19. [ ] `benches/metrics/`：token_f1 / bleu / multiple_choice_accuracy / recall_at_k / mrr（纯标准库，单测）
20. [ ] `benches/judge.py`：OpenAI 兼容 judge 客户端，`from_env()` 无凭据返回 None；严格判定 prompt；超时/重试；记录模型名与计数
21. [ ] `benches/datasets.py`：`DATASET_SPECS` + `resolve_dataset`（真实优先/fixture 回退，标 dataset_source）+ `download`（urllib，失败打印手动指引）
22. [ ] 四基准 fixture（极小合成，结构同上游）：`lococo_refined` / `halumem` / `longmemeval` / `personamem`
23. [ ] 重构 `track_b/` 为注册表：`BENCHMARKS`=四基准；每基准子包导出 `NAME`/`DATASET_FILES`/`load`/`run`；`run_track_b` 分派+聚合+报告（保留顶层展平 systems）
24. [ ] `adapters/base.py` 扩展可选能力 `list_memories`/`update`+`UnsupportedCapability`；`aria_memo.py` 对接 CLI 新能力；其余系统默认降级
25. [ ] `benches/tests/`：`test_metrics` / `test_loaders` / `test_track_b`（fake backend+fake judge 端到端，judge=None 时 skip）/ `test_adapters`（降级）/ `test_datasets`
26. [ ] `run.py`：`--benchmarks` 默认四基准、移除 beam；新增 `--download` / `--limit` / `--judge-model`；汇总打印
27. [ ] 文档同步：`benches/README.md` / `data/README.md` / `docs/compare.md` / `docs/bench_results.md` / `AGENTS.md`
28. [ ] 验收：`cargo test` + `cargo clippy --all-targets` 全绿；`python -m pytest benches/` 离线全绿；`run.py --bench locomo_refined --ingest-only` 等可跑

## M4 — 多关系记忆平面（Jev-Mem 启发，已落地）

> 需求：在扁平 `MemoStore` 之上叠加四视图有向关系边（semantic/temporal/causal/entity），local-first 离线默认，可插拔 LLM scorer；write→connect 推断边，retrieve→assess→expand 有界图遍历 + 可检视 `RetrieveTrace`。规格见 requirements.md §1.3 / §2.5–§2.8 / §3 / §4 / §5.1。

29. [x] `memo-core` 模型：新增 `RelationKind`/`Relation`/`GraphRetrieveQuery`/`RetrieveTrace`/`GraphRetrieveResult` + `graph_bfs`（BFS + 去环，0.9/hop 衰减，budget 封顶）；`Relation::validate` 拒自环 / score 越界 / 空 provenance
30. [x] `memo-core` trait：`MemoStore` 扩展 `add_relation`/`get_relations`/`delete_relations`/`expand`；in-memory 测试实现 + 单测（含 `expand` 拒绝非法 query）
31. [x] `memo-storage`：新增独立 `relations` 表（PK `from_id`+`to_id`+`kind`）+ 索引（`idx_relations_from`/`idx_relations_to`）；关系 CRUD + 有界 BFS `expand` 实现
32. [x] `memo`：`MemoryController` + `RelationScorer` trait（默认 `LocalRelationScorer` 离线：semantic=cosine / temporal=时间序 / entity=token Jaccard / causal=时间邻+重叠）；`connect`（write→connect，对称视图双向建边，temporal 单向）/ `retrieve`（hybrid 选种 → 跨 views 扩展 → 打分 + Trace）；`RelationConfig` 控制阈值 / 候选数 / 每写最大边数
33. [x] `memo/lib`：re-export 新类型与 `MemoryController`
34. [x] CLI：新增 `connect`/`relate`/`relations`/`graph` 子命令 + `search --graph`；`--json` 输出含 `trace`；`commands.rs`/`main.rs` 正常 + 异常单测
35. [x] 单测覆盖（正常 + 异常）：自环 / score 越界 / 空 provenance、未知 kind、seeds 空 / budget=0 / max_hops=0 / top_k=0、端点缺失 `NotFound`、`delete` 过滤、`expand` budget/hops/去环/衰减、views 空全开、`LocalRelationScorer` ∈ [0,1]、CLI 拒非法；扁平 `search`/`recall` 回退不破坏
36. [x] 验收：`cargo test` 全绿、`cargo clippy --all-targets` 无告警；提交 `feat(memory): add multi-relational memory plane (Jev-Mem inspired)`（`13ab4ec`）

## M5 — 检索优化：FTS5 词法下推 + 批量检索（已落地）

> 需求：提高 `keyword_weight` 让词法命中更早剪枝候选；把词法粗排下推到 SQLite（FTS5 `bm25()` 候选集）；提供批量检索入口。`keyword_weight` 默认 `0.3 → 0.5`；rusqlite 启用 `fts5` feature。规格见 requirements.md §1.1 / §2.2 / §2.3 / §2.4 / §3。

37. [x] `Cargo.toml`（workspace）：rusqlite 启用 `features = ["bundled", "fts5"]`
38. [x] `memo-core`：`SearchQuery::new` 默认 `keyword_weight` 由 `0.3` 改为 `0.5`（语义/词法权重 `0.7`/`0.5`）
39. [x] `memo-storage`：SCHEMA 新增独立 `memories_fts`（`content` 索引列 + `mem_id UNINDEXED`）；`migrate` 以 `id NOT IN (SELECT mem_id FROM memories_fts)` 幂等回填；`add`/`update`(先删后插)/`forget` 同步 FTS5 行
40. [x] `memo-storage`：重写 `search` 走 `search_inner` —— `keyword_weight>0 && 非空 query` 时用 FTS5 `MATCH` + `bm25()` 取 `top_k*5 max 50` 候选（`fts5_match_expr` 分词 OR、CJK/标点丢弃），仅对候选集做 Rust 内存余弦精排；否则全表扫描保语义召回；`search_batch` 持锁循环 `search_inner`
41. [x] `memo-core` trait：`MemoStore` 增 `search_batch` 默认方法（逐条 `search`，后端可重写）
42. [x] `memo`：`MemoManager::search_batch`（逐条 embed 后单锁评分）与 `recall_batch`（构造纯向量 `SearchQuery` 复用 `search_batch`）
43. [x] CLI：新增 `search-batch` 子命令（重复 `--text` 传多 query，支持 `--json` 二维数组）；`bench` 增 `--batch` 单次锁内批量吞吐埋点（`report["batch"]` 含 `total_ms`/`ops_per_sec`）
44. [x] 单测覆盖（正常 + 异常）：sqlite `list_and_search`/`semantic_search_ranks_by_cosine`/`lexical_relevance_lifts_specific_fact` 验证 FTS5 下推与纯语义回退；`commands.rs` bench 测试已传 `batch=false`（top_k=0 / size=0 走 `InvalidParam`）；`search_batch`/`recall_batch` 路径经 manager/storage 既有用例覆盖
45. [x] 验收：`cargo test` 全绿、`cargo clippy --all-targets` 无告警；提交 `perf(search): push lexical candidate pruning to FTS5 + add batch retrieval`

### M5
- `cargo test` + `cargo clippy --all-targets` 全绿（含 FTS5 下推与批量用例）。
- 黄金路径：`add` 含关键词记忆 → `search --text "rust"` 经 FTS5 命中且关键词记忆排名第一；纯语义 query（embedder 命中）在词法无命中时仍能全表召回。
- 批量：`search-batch --text "a" --text "b"` 返回与输入等长的结果组；`bench --batch --json` 输出 `report["batch"]`。
- 回退：既有扁平 `search`/`recall` 与 `Memo` 模型行为不变（语义召回不依赖 FTS5）。

## M6 — Track A 扩展：规模曲线 / 本地控制组 / A2 方差 / 写长尾（已落地）

> 需求：放大 size（10k/100k）看延迟与 p99 增长曲线验证亚线性；补控制组 sqlite_vec、chromem 进 track_a 让对比矩阵有真实数值；扩大 A2 查询集（≥50–100 条、复用 Track B 真实语料）给出有统计意义的 recall/MRR 与方差；压测写长尾（批量嵌入、WAL、事务合并）看 add p99 是否收敛。规格见 requirements.md §6.3 / §6.5 / §6.6 / §6.8。

46. [x] Rust 写长尾基座：`Embedder::embed_batch` 默认方法（traits.rs）+ `LocalEmbedder::embed_batch`；`MemoManager::add_batch`（批量嵌入+事务合并）+ `SqliteStore::enable_wal`（`PRAGMA journal_mode=WAL`，trait 默认 no-op）；`MemoStore::enable_wal` 在 `SqliteStore` 覆写。
47. [x] CLI `bench` 新增 `--wal` / `--batch-embed` / `--bulk`（`BenchConfig` 收口），报告 `add_baseline` / `add_wal` / `add_batch_embed` / `add_bulk` 四分段 p50/p99/ops；`size=0`/`top_k=0` 仍 `InvalidParam`。
48. [x] 控制组 adapter：`benches/adapters/sqlite_vec.py`（pip `sqlite-vec` + 离线哈希-n-gram 嵌入）、`benches/adapters/chromem.py`（chromem-go 子进程），均 lazy import、缺依赖/二进制则 `SkipBackend`/skip 并写 `reason`；`build_backend` 注册 `sqlite_vec`/`chromem`，默认 `--systems` 纳入。
49. [x] `metrics/retrieval.py` 增 `mean_std`（样本标准差）；`benches/data/synthetic_retrieval_v2.json`（84 条、关键词/同义改写/干扰项混合）；`track_a/datasets.py` 加载 `synthetic`/`synthetic_v2`/`track_b:locomo_refined`/`track_b:halumem`（缺数据 skip）。
50. [x] `run_retrieval_quality` 支持 `dataset` 选择、按查询集计算 Recall@k/MRR 及方差（`recall_at_k_std`/`mrr_std`/`n_queries`）；`run_microbench` 改多尺寸 sweep（默认 1k/10k/100k）并输出 `scaling`（p99 增长因子 + 亚线性/线性/超线性判定）；`run.py` 增 `--sizes` / `--a2-dataset` 透传。
51. [x] 单测（离线、零网络）：`mean_std` 空/单值/样本方差；控制组缺依赖 skip；`load_dataset` 各源（含 track_b 真实 fixtures）；`run_retrieval_quality` 经 fake backend 出方差；`run_microbench` 多尺寸 + scaling 条目。
52. [x] 验收：Rust `cargo test` + `cargo clippy --all-targets` 全绿（含 embed_batch/add_batch/enable_wal/WAL 生效单测）；benches `python -m unittest tests.test_metrics tests.test_adapters tests.test_track_a` 全绿；`run.py --track a --sizes 20 --a2-dataset synthetic_v2` 端到端冒烟通过。

### M6
- `run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem` 产出 A1 多尺寸 `scaling`（p99 增长因子 + 亚线性判定）与 A2 `recall_at_k`/`mrr` ± 方差。
- `aria-memo bench --wal --batch-embed --bulk --json` 输出 `add_baseline`/`add_wal`/`add_batch_embed`/`add_bulk` 四分段，验证 add p99 收敛。
- 控制组与 Track B 复用集在依赖/数据缺失时 skip 并写 `reason`，不伪造数值。

## 验证

### M1
- `cargo test`：正常 + 异常路径全绿。
- `cargo clippy --all-targets`：无告警。
- 黄金路径 / 异常单测 / wasm32 编译。
- 纯向量召回 `recall` 与混合检索 `search` 单测全绿；`recall` 语义相似记忆排名第一。

### M2
- `docs/compare.md` 维度齐全。
- `cargo run -p aria-memo -- bench --size 100 --json` 输出合法 JSON。
- `python benches/run.py --track a --size 100` 写出 `benches/results/`。
- `python benches/run.py --track b --dry-run` 走通骨架；缺密钥 skip 并写原因。

### M4
- `cargo test` + `cargo clippy --all-targets` 全绿（含关系平面用例）。
- 黄金路径：`add → connect` 推断四视图边 → `relations` 列边 → `graph`（或 `search --graph`）跨视图扩展返回打分记忆 + `trace`。
- 异常路径：自环 / score 越界 / 空 provenance / 未知 kind / seeds 空 / budget=0 / max_hops=0 / top_k=0 / 端点缺失 `NotFound` / `delete` 过滤全空 `InvalidParam` 均有单测。
- 回归：既有扁平 `search`/`recall` 与 `Memo` 模型行为不变。
- 手动冒烟：`cargo run -p aria-memo -- connect --id <id>`、`relate --from .. --to .. --kind semantic`、`relations --json`、`graph --text "..." --json`。
