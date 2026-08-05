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

## M2 — 业界功能/性能评测（A + B）

> 对比系统：mem0 / MemOS / MemPalace / Zep / Letta。编排工程：`benches/`（Python），不采用 `crates/bench`。

10. [x] `requirements.md` §6 增补 Track A/B、矩阵、验收；本清单同步
11. [x] `docs/compare.md` 功能对比矩阵（aria + 五系统）
12. [x] CLI `memo bench --json`（进程内 add/search 微基准，供 Python 解析）
13. [x] `benches/` 脚手架：`common` / `adapters` / `track_a` / `track_b` / `data` / `results` / `run.py`
14. [x] Track A：微基准汇总（1k/10k）+ 合成检索 Recall@k / MRR（aria 必跑；他系统按可用性）
15. [x] Track B：LoCoMo / LongMemEval / BEAM runner 骨架 + dry-run；五系统 adapter 接口齐全
16. [x] 文档：`benches/README.md`、`docs/bench_results.md`、根 README / AGENTS 链到评测

## 验证

### M1
- `cargo test`：正常 + 异常路径全绿。
- `cargo clippy --all-targets`：无告警。
- 黄金路径 / 异常单测 / wasm32 编译。

### M2
- `docs/compare.md` 维度齐全。
- `cargo run -p aria-memo -- bench --size 100 --json` 输出合法 JSON。
- `python benches/run.py --track a --size 100` 写出 `benches/results/`。
- `python benches/run.py --track b --dry-run` 走通骨架；缺密钥 skip 并写原因。
