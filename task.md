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
