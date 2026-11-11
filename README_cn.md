<div align="center">
<a name="readme-top"></a>

# memo

**面向 LLM Agent 的本地优先长期记忆存储**

Rust 实现的边缘/移动端部署：记忆的增删改查、语义 + 关键词混合检索、巩固、去重与遗忘——<br/>
零网络依赖、不引入重型 ML 框架。还内置默认离线的多关系记忆平面<br/>
（semantic / temporal / causal / entity 四视图）。

[English](./README.md) · **简体中文** · [文档](./docs)

[![][release-shield]][release-link] [![][rust-shield]][rust-link] [![][license-shield]][license-link] [![][lastcommit-shield]][lastcommit-link]
<br/>
[![][storage-shield]][storage-link] [![][tests-shield]][tests-link] [![][docs-shield]][docs-link] [![][i18n-shield]][i18n-link]
</div>

<details>
<summary><kbd>Table of contents</kbd></summary>

#### TOC

**上手**

- [✨ 功能](#features)
- [💾 架构](#architecture)
- [🔍 检索](#search--retrieval)
- [🚀 快速开始](#quick-start)
- [🧰 命令行](#command-line)

**内部机制与数据**

- [🕸️ 多关系记忆平面](#multi-relational-memory-plane)
- [📊 对比评测](#benchmarks--comparison)
- [📂 目录](#directory)

**信任与治理**

- [🔏 信任库安全](#trust-store-security)
- [🔨 开发](#development)
- [📚 文档](#documentation)
- [📝 许可证](#license)
- [🤝 工程规范](#engineering-conventions)

</details>

## ✨ 功能

memo 是一个本地优先、默认离线的 LLM Agent 长期记忆存储，用纯 Rust 实现，面向边缘与移动端部署。

- 📦 **增删改查与生命周期** —— 记忆的创建、读取、更新、遗忘，以及扁平记忆库上的巩固与去重。
- 🔍 **混合检索** —— 语义（本地嵌入余弦）与词法（关键词）相关性融合；词法下推在余弦计算前先剪枝候选集。
- 🕸️ **多关系平面** —— 在扁平存储之上叠加四视图关系图（semantic / temporal / causal / entity），默认离线，可插拔 LLM scorer。
- 🌐 **多语言切分** —— 中文 `zh/zh-TW` 用 `jieba-rs`、日语/韩语 `ja/ko` 用 `lindera`、其余按空格切分，覆盖 cockpit 全部 13 种语言，使子词在 FTS5 中可命中。
- 🚫 **零网络依赖** —— 运行时无出向调用；唯一的网络路径是可选的 `upgrade` 下载，且被钉在安全的信任库上。
- 🦀 **纯 Rust，无重型 ML** —— 本地 hash/TF-IDF 嵌入器，无 PyTorch/TensorFlow，轻量可嵌入。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 💾 架构

分层 cargo workspace，trait 解耦，依赖方向单一：

```
cli(aria-memo) → memo(编排) → storage(SQLite) / embed(本地嵌入) → core(模型/错误/trait)
```

`memo` 依赖 `core` + `storage` + `embed`；`storage` / `embed` 只依赖 `core`。不存在上向依赖。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔍 检索

混合检索融合语义（本地嵌入余弦）与词法（关键词）相关性：
`score = semantic_weight·cosine + keyword_weight·lexical_relevance`。

- **词法下推（FTS5）。** `memories.content` 由 SQLite FTS5 虚拟表 `memories_fts` 索引。混合查询时，存储层先在 *SQLite 内部* 跑 `MATCH` + `bm25()` 把候选集剪枝到词法命中最高的若干条，再仅对这小集合计算余弦——彻底消除了旧有的全表扫描。纯语义查询（或 FTS5 无词法命中）回退到全表扫描，保证召回不丢。**多语言词级切分：** 由于本项目的 bundled SQLite 无法开启内置 `icu` 分词器，写入前会在 Rust 侧做词级预切分、查询侧用同一 `segment` 逻辑切分，使子词命中而非整段当一个不可匹配的 token。路由按 Unicode 文字范围：中文 `zh/zh-TW` 用 `jieba-rs`，日语 `ja` 与韩语 `ko` 用 `lindera`（内嵌 ipadic/ko-dic 词典），其余按空格切分的语言（en/es/fr/de/it/ru/pt/ar/hi）走空格切分——覆盖 cockpit 的全部 13 种语言。FTS 索引在打开时通过 `PRAGMA user_version` 一次性重建（多语言路由已升到 4），使已有数据库也用上切分后的 token。
- **更高的词法权重。** `SearchQuery.keyword_weight` 默认 `0.5`（原 `0.3`），词法命中更早参与剪枝与排序；`semantic_weight` 保持 `0.7`。`MemoryController` 同样使用 `0.7 / 0.5`。
- **批量检索。** `MemoStore::search_batch` / `MemoManager::search_batch` / `recall_batch` 在单次连接锁内（一次 FTS5 准备 + 一次扫描）对多个查询打分，返回 `Vec<Vec<ScoredMemo>>`。CLI 提供 `aria-memo search-batch --text … --text …` 与 `aria-memo bench --batch`（报告 `batch.ops_per_sec`）。

**离线检索质量。** 扁平 `search` 走混合检索：本地语义分（哈希嵌入的余弦）+ 零依赖的词法分（`lexical_relevance`：按稀有度加权的词项重叠 + 精确短语奖励，并含 CJK 字符 2-gram）。由于内置嵌入器是轻量 hash/TF-IDF 向量器（不引入重型 ML），语义信号刻意偏弱；`lexical_relevance` 最大化关键词贡献，使离线召回仍可用。可用 `SearchQuery.keyword_weight`（默认 `0.5`）调节平衡。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🚀 快速开始

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "用户喜欢 Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

`cargo test` 无需 Postgres、网络或凭据即可通过——全部基于本地 SQLite。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🧰 命令行

`aria-memo` 提供完整的记忆生命周期命令，外加两个自管理命令（对齐 `aria-router`）：

| 命令 | 主要参数 | 用途 |
|---|---|---|
| `add` | `--type` `--content` `--importance`（0.5） | 写入记忆 |
| `get` | `--id` | 读取单条记忆 |
| `search` | `--text` `--top-k`（5） `--json` `--graph` | 混合检索（可开启图感知） |
| `recall` | `--text` `--top-k`（5） | 纯向量召回 |
| `search-batch` | `--text`（可重复） | 单次锁内批量混合检索 |
| `list` | `--json` | 列出记忆 |
| `update` | `--id` `--content` | 更新记忆（为 HaluMem 提供操作级钩子） |
| `forget` | `--id` | 遗忘记忆 |
| `bench` | `--size` `--batch` `--json` | 吞吐基准 |
| `connect` | `--id` | 为已存记忆推断四视图边 |
| `relate` | `--from` `--to` `--kind` `--score` | 手动建边 |
| `relations` | `--from` `--to` `--kind` `--json` | 列边 |
| `graph` | `--text` `--views` `--budget` `--max-hops` `--top-k` `--json` | 有界多关系遍历 |
| `setup` | `--status` `--clear` | 持久化 CLI 配置（`~/.ariacompute/memo-cli.yml`） |
| `upgrade` | `[version]` `--url` | 从 GitHub/Gitee Releases 替换二进制 |
| `--version` / `-v` | — | 打印构建版本 |

**自管理。** `aria-memo setup` 将 CLI 配置持久化到 `~/.ariacompute/memo-cli.yml`（即 `upgrade_url`，Releases 组织根）。交互运行时会提示选择 **GitHub**（`https://github.com/ariacompute`，默认）或 **Gitee**（`https://gitee.com/ariacompute`）。非交互运行时遵循 `ARIA_MEMO_SITE=cn`（→ Gitee），否则沿用已有配置，最后回落到 GitHub 默认。`setup --status` 打印配置路径与 `upgrade_url`；`setup --clear` 删除配置文件。

`aria-memo upgrade [version]` 从 GitHub/Gitee Releases 下载并原地替换当前二进制（不传版本升级到最新稳定版，传 `version` 升级到指定 tag）。`upgrade --url <URL>` 本次运行覆盖配置中的 `upgrade_url`。`upgrade_url` 解析优先级：`--url` > `~/.ariacompute/memo-cli.yml` > `ARIA_MEMO_UPGRADE_URL` 环境变量 > 内置默认（`https://github.com/ariacompute`）。Release 资产命名约定：`aria-memo_{ver}_{os}.tar.gz`（`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`，Windows 为 `.zip`）。

> home 目录 `~/.ariacompute` 可由 `ARIA_COMPUTE_HOME` 覆盖。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🕸️ 多关系记忆平面

在扁平 `MemoStore` 之上构建的结构化多关系记忆层——零网络依赖、默认离线（可插拔 LLM `RelationScorer`）。

四种关系视图以有向边连接记忆：

- `semantic` — 语义关联（嵌入余弦）
- `temporal` — 时间序 / 共现
- `causal` — 因果链
- `entity` — 同一实体聚合

边存于独立的 `relations` 表，绝不改动 `Memo`。写入时 `connect` 推断四视图边；读取时 `retrieve` 先做混合检索选种子锚点，再跨视图做有界图扩展，返回打分记忆 + 可检视 `RetrieveTrace`（对齐 Jev-Mem 的 Retrieve→Assess→Expand 与透明决策）。

```bash
# 为已存记忆推断四视图边
cargo run -p aria-memo -- connect --id <id>

# 手动建边
cargo run -p aria-memo -- relate --from <id> --to <id> --kind semantic --score 0.8

# 列边（按 from / to / kind 过滤）
cargo run -p aria-memo -- relations --kind semantic --json

# 有界多关系遍历（views/budget/max-hops/top-k）
cargo run -p aria-memo -- graph --text "Rust 系统编程" --views semantic,entity --top-k 20 --json

# 或在普通 search 上开启图感知
cargo run -p aria-memo -- search --text "Rust" --graph
```

默认 `LocalRelationScorer` 完全本地（semantic = 余弦，temporal = 时间序，entity = token Jaccard，causal = 时间邻 + 重叠）。可换 LLM 实现而不动控制器。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📊 对比评测

对比系统：mem0 / MemOS / MemPalace / Zep / Letta，外加本地嵌入式**控制组** `sqlite_vec` 与 `chromem`（默认纳入；其依赖/二进制缺失时按 `reason` 跳过并写原因，绝不伪造数值）。

- 功能矩阵：[docs/compare.md](./docs/compare.md)
- 评测说明与结果：[docs/bench_results.md](./docs/bench_results.md)
- Python 编排（Track A 存储/检索 + Track B 端到端质量）：[benches/README.md](./benches/README.md)

```bash
pip install -r benches/requirements.txt

pushd benches/chromem-wrapper
go get github.com/philippgille/chromem-go@latest && go mod tidy && go build -o chromem .
export CHROMEM_BIN="$PWD/chromem"
popd

python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
python benches/run.py --track b --download
```

### Track A 评测报告

运行：`python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem`（结果位于 `benches/results/`）。本报告数据：
- aria：`python benches/run.py --track a --sizes 1000,10000,100000 --systems aria`
- sqlite_vec：`python benches/run.py --track a --sizes 1000,10000,100000 --systems sqlite_vec`
- chromem：`python benches/run.py --track a --sizes 1000,10000,100000 --systems chromem`

**A1 — 微基准（top_k=5，离线；aria `add_baseline` = 逐条嵌入 + 逐条事务）：**

| 系统 | 尺寸 | 分段 | ops/秒 | p50 (ms) | p99 (ms) | 说明 |
|------|------|------|-------:|---------:|---------:|------|
| aria | 1000 | `add_baseline` | 157.18 | 5.91 | 13.35 | 朴素逐条路径（基线） |
| aria | 1000 | `search` | 1456.11 | 0.69 | 0.69 | 单 query 均摊 |
| aria | 10000 | `add_wal` | 752.06 | 1.06 | 4.08 | WAL 日志模式 |
| aria | 10000 | `add_batch_embed` | 40039.03 | 0.02 | 0.02 | 批量嵌入 + 事务写入 |
| aria | 10000 | `add_bulk` | 72164.68 | 0.01 | 0.01 | 逐条嵌入，事务合并 |
| aria | 10000 | `search` | 140.71 | 7.11 | 7.11 | 单 query 均摊 |
| aria | 100000 | `add_batch_embed` | 41459.87 | 0.02 | 0.02 | 100k 仅测此优化分段 |
| aria | 100000 | `search` | 13.65 | 73.25 | 73.25 | 单 query 均摊（全语料扫描） |
| sqlite_vec | 1000 | `add` | 23345.52 | 0.04 | 0.04 | 批量插入 |
| sqlite_vec | 1000 | `search` | 592.15 | 1.69 | 1.69 | 单 query 均摊 |
| sqlite_vec | 10000 | `add` | 22223.95 | 0.04 | 0.04 | 批量插入 |
| sqlite_vec | 10000 | `search` | 50.42 | 19.84 | 19.84 | 单 query 均摊 |
| sqlite_vec | 100000 | `add` | 20212.36 | 0.05 | 0.05 | 批量插入 |
| sqlite_vec | 100000 | `search` | 4.58 | 218.25 | 218.25 | 单 query 均摊（全语料扫描） |
| chromem | 1000 | `add` | 22126.47 | 0.05 | 0.05 | 批量插入（`add-batch`） |
| chromem | 1000 | `search` | 208.95 | 4.79 | 4.79 | 单 query 均摊（`query-batch`） |
| chromem | 10000 | `add` | 23672.96 | 0.04 | 0.04 | 批量插入（`add-batch`） |
| chromem | 10000 | `search` | 23.08 | 43.34 | 43.34 | 单 query 均摊（`query-batch`） |
| chromem | 100000 | `add` | 23089.77 | 0.04 | 0.04 | 批量插入（`add-batch`） |
| chromem | 100000 | `search` | 2.33 | 429.42 | 429.42 | 单 query 均摊（`query-batch`） |

**A2 — 检索质量（synthetic_v2，84 条查询，top_k=5）：**

| 系统 | Recall@5 | MRR | 查询数 | 离线 |
|------|---------:|----:|-------:|-----:|
| aria | 1.00 | 1.00 | 84 | true |
| sqlite_vec | 0.9762 | 0.9762 | 84 | true |
| chromem | 1.00 | 1.00 | 84 | true |

### Track B 评测报告

四个基准的端到端记忆质量评测：`locomo_refined`、`halumem`、`longmemeval`、`personamem`。aria 后端通过 CLI 驱动（先构建：`cargo build -p aria-memo --release`）。

```bash
# 可选：打分前先验证数据集加载与后端能力
python benches/run.py --track b --dry-run

# 真实数据集（自动下载；网络不通时明确报错并给出手动指引）
python benches/run.py --track b --download
```

离线指标（F1/BLEU、多选、检索 Recall@k、抽取 Recall）**无需 LLM** 即可运行。LLM judge 指标为**可选**：无凭据时静默跳过：

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://tokenhub.tencentmaas.com
python benches/run.py --track b --download --judge-model hy3
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

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📂 目录

- `crates/core` — 数据模型、统一错误 `MemoError`、trait
- `crates/embed` — 本地轻量 embedder（ngram + 哈希/TF-IDF 向量）+ 余弦相似度
- `crates/storage` — rusqlite 嵌入式持久化后端
- `crates/memo` — 记忆管理编排与生命周期（含多关系 `MemoryController` / `RelationScorer`）
- `crates/cli` — 命令行入口
- `benches/` — 业界对比评测
- `docs/` — 功能矩阵与评测结果说明

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔏 信任库安全

2026-10 的 eMudhra（emSign）中间 CA 私钥泄露事件由三层兜底：

- **Rust CLI（`aria-memo`）。** 出向 HTTPS（自管理的 `upgrade --url` 发布下载及未来任何 HTTP 拉取）走 `reqwest` + `rustls-tls`，其信任库是编译进二进制的 **`webpki-roots`**，已在 `crates/cli/Cargo.toml` 钉住 `>=1.0.10` 并在 `Cargo.lock` 升 `1.0.9 → 1.0.10`——1.0.10+ 移除了被泄露的 emSign 根。二进制**不**使用系统 bundle，故锁升级是决定性修复。
- **Python 评测（`benches/`）。** 编排可能发起出向 TLS（数据集下载、LLM judge API）；`benches/requirements.txt` 已钉 `certifi`，使其随附 CA 库为 emSign 移除后的版本。
- **离线检测器。** `scripts/check-emudhra.sh` 按已知 emSign 根指纹扫描主机/系统 CA bundle：

```bash
scripts/check-emudhra.sh /etc/ssl/certs/ca-certificates.crt
```

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔨 开发

| 目标 | 作用 |
|---|---|
| `cargo test` | `cargo test --workspace` |
| `cargo test -p memo-core` | core crate 单测 |
| `cargo build` | 构建全部 crate |
| `cargo run -p aria-memo -- <cmd>` | 运行 CLI |
| `cargo clippy --all-targets` | 对所有 target 做 lint |
| `cargo fmt --all -- --check` | 校验格式 |
| `scripts/check-emudhra.sh <bundle>` | 离线扫描 eMudhra/emSign CA 指纹 |

`cargo test` 无需 Postgres、网络或凭据即可通过——每个外部依赖都是本地（SQLite）或被 fake。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📚 文档

完整细节文档位于 [`docs/`](./docs)：

| 文档 | 覆盖内容 |
|---|---|
| [功能矩阵](./docs/compare.md) | memo vs mem0 / MemOS / MemPalace / Zep / Letta + 本地控制组 |
| [评测结果](./docs/bench_results.md) | Track A/B 方法论与结果 |
| [评测编排](./benches/README.md) | Python 编排（Track A 存储/检索 + Track B 端到端） |
| [需求规格](./requirements.md) | 功能边界 / 异常 / 验收标准 |
| [任务清单](./task.md) | 实施清单 |
| [AGENTS.md](./AGENTS.md) | Agent 工程上下文入口与目录索引 |

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📝 许可证

Apache-2.0，详见 [LICENSE](./LICENSE)。

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🤝 工程规范

本仓库遵循 Harness Engineering：仓库本身即 Agent 的上下文，治理它的文档与代码一同版本化。

- [`AGENTS.md`](./AGENTS.md) —— Agent 工程上下文入口与目录索引
- [`requirements.md`](./requirements.md) —— 需求规格（功能边界 / 异常 / 验收标准），人审通过后再编码
- [`task.md`](./task.md) —— 实施清单

<div align="right">

[![][back-to-top]](#readme-top)

</div>

---

<div align="center">

Copyright © 2026 [ariacompute][org]. <br/>
This project is [Apache-2.0](./LICENSE) licensed.

</div>

<!-- 链接定义 -->

[org]: https://github.com/ariacompute

[release-shield]: https://img.shields.io/github/v/release/ariacompute/memo?style=flat-square&labelColor=black&color=555
[release-link]: https://github.com/ariacompute/memo/releases
[rust-shield]: https://img.shields.io/badge/language-Rust-555?style=flat-square&labelColor=black&color=555
[rust-link]: ./Cargo.toml
[license-shield]: https://img.shields.io/github/license/ariacompute/memo?style=flat-square&labelColor=black&color=555
[license-link]: ./LICENSE
[lastcommit-shield]: https://img.shields.io/github/last-commit/ariacompute/memo?style=flat-square&labelColor=black&color=555
[lastcommit-link]: https://github.com/ariacompute/memo/commits

[storage-shield]: https://img.shields.io/badge/storage-SQLite%2BFTS5-555?style=flat-square&labelColor=black&color=555
[storage-link]: ./crates/storage
[tests-shield]: https://img.shields.io/badge/tests-offline_first-555?style=flat-square&labelColor=black&color=555
[tests-link]: ./Cargo.toml
[docs-shield]: https://img.shields.io/badge/docs-2_guides-555?style=flat-square&labelColor=black&color=555
[docs-link]: ./docs
[i18n-shield]: https://img.shields.io/badge/tokenize-13_locales-555?style=flat-square&labelColor=black&color=555
[i18n-link]: #search--retrieval

[back-to-top]: https://img.shields.io/badge/-BACK_TO_TOP-151515?style=flat-square
