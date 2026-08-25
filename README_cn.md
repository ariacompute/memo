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

### 评测报告

本地运行：`cargo run -p aria-memo -- bench --size 1000 --json`
（本地优先、离线、零网络）。

| 操作    | 数据量 | ops/秒 | p50 (ms) | p99 (ms) |
|---------|-------:|-------:|---------:|---------:|
| `add`   | 1000   | 161.18 | 5.15     | 17.76    |
| `search`| 1000   | 79.88  | 11.74    | 25.58    |

> 环境：本地优先嵌入式存储（rusqlite bundled），无网络，`top_k=5`，warmup=10。
> 数值为单次开发机运行的示例；请自行重新编译运行以获取你的硬件数据。

### Track A 评测报告

运行：`python benches/run.py --track a --size 1000`
（结果位于 `benches/results/`，例如 `20260825T054023Z/track_a.json`）。

**A1 — 微基准（aria-memo，size=1000，top_k=5，离线）：**

| 操作    | ops/秒 | p50 (ms) | p99 (ms) |
|---------|-------:|---------:|---------:|
| `add`   | 177.60 | 4.79     | 18.04    |
| `search`| 636.91 | 1.47     | 3.00     |

**A2 — 检索质量（synthetic_retrieval.json，8 条查询，top_k=5）：**

| 系统  | Recall@5 | MRR  | 查询数 | 离线  |
|-------|---------:|-----:|-------:|:-----:|
| aria  | 1.00     | 1.00 | 8      | true  |

> A1 复用进程内 CLI `bench` JSON；A2 在合成数据集上衡量混合（语义 + 关键词）检索质量。完整对比矩阵见 [docs/compare.md](./docs/compare.md)。

### Track B 评测报告

四个基准的端到端记忆质量评测：`locomo_refined`、`halumem`、`longmemeval`、`personamem`。
aria 后端通过 CLI 驱动（先构建：`cargo build -p aria-memo --release`）。

```bash
# 可选：打分前先验证数据集加载与后端能力
python benches/run.py --track b --dry-run

# 真实数据集（自动下载；网络不通时明确报错并给出手动指引）
python benches/run.py --track b --download
```

离线指标（F1/BLEU、多选、检索 Recall@k、抽取 Recall）**无需 LLM** 即可运行。
LLM judge 指标为**可选**：无凭据时静默跳过：

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://...   # 可选，OpenAI 兼容
python benches/run.py --track b --judge-model gpt-4o-mini
```

> 各基准在缺少真实数据时会回退到仓库内置 `benches/data/fixtures/`（合成样本，离线冒烟），报告中以 `dataset_source` 标注。可用 `--benchmarks`、`--limit`、`--ingest-only` 限定评测范围。

**限定大型真实数据集。** 真实数据集可能非常大——例如 `halumem`（HaluMem-Medium）约需 7.5 万次 `add` 调用。`--limit N` 限定每个基准**注入与评测**的样本数（对 `halumem` 每个记录为一个样本集；对 `locomo_refined`/`longmemeval`/`personamem` 则限制问题/条目数）。省略 `--limit` 时，默认对每个基准施加 `50` 样本上限（会打印到 stderr），避免无界运行卡死。注入过程会向 stderr 输出进度（`[bench] ingest 500/N ... ingest done`）。

```bash
# 有界的快速冒烟运行
python benches/run.py --track b --limit 2
# 全量运行（耗时但受上限约束）；构建 release 二进制可加速每次 add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
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
