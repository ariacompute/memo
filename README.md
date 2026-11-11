<div align="center">
<a name="readme-top"></a>

# memo

**Local-first long-term memory store for LLM Agents**

Rust-built for edge/mobile deployment: CRUD, semantic + keyword hybrid retrieval,<br/>
consolidation, deduplication, and forgetting — with zero network dependency and<br/>
no heavy ML frameworks. Ships an offline-first, multi-relational memory plane<br/>
(four relation views: semantic / temporal / causal / entity).

**English** · [简体中文](./README_cn.md) · [Docs](./docs)

[![][release-shield]][release-link] [![][rust-shield]][rust-link] [![][license-shield]][license-link] [![][lastcommit-shield]][lastcommit-link]
<br/>
[![][storage-shield]][storage-link] [![][tests-shield]][tests-link] [![][docs-shield]][docs-link] [![][i18n-shield]][i18n-link]
</div>

<details>
<summary><kbd>Table of contents</kbd></summary>

#### TOC

**Get started**

- [✨ Features](#features)
- [💾 Architecture](#architecture)
- [🔍 Search & retrieval](#search--retrieval)
- [🚀 Quick start](#quick-start)
- [🧰 Command line](#command-line)

**Internals & data**

- [🕸️ Multi-relational memory plane](#multi-relational-memory-plane)
- [📊 Benchmarks & comparison](#benchmarks--comparison)
- [📂 Directory](#directory)

**Trust & governance**

- [🔏 Trust store security](#trust-store-security)
- [🔨 Development](#development)
- [📚 Documentation](#documentation)
- [📝 License](#license)
- [🤝 Engineering conventions](#engineering-conventions)

</details>

## ✨ Features

memo is a local-first, offline-by-default long-term memory store for LLM agents, built in pure Rust for edge and mobile deployment.

- 📦 **CRUD + lifecycle** — Create, read, update, and forget memories, plus consolidation and deduplication over a flat memory store.
- 🔍 **Hybrid retrieval** — Semantic (cosine over local embeddings) blended with lexical relevance (FTS5 keyword + BM25), with lexical pushdown that prunes candidates before the cosine pass.
- 🕸️ **Multi-relational plane** — A four-view relation graph (semantic / temporal / causal / entity) layered on top of the flat store; offline by default with a pluggable LLM scorer.
- 🌐 **Multilingual segmentation** — `zh`/`zh-TW` via `jieba-rs`, `ja`/`ko` via `lindera`, the rest whitespace; covers all 13 cockpit locales so sub-words match in FTS5.
- 🚫 **Zero network dependency** — No outbound calls at runtime; the only network path is the optional `upgrade` download, pinned to a safe trust store.
- 🦀 **Pure Rust, no heavy ML** — A local hash/TF-IDF embedder; no PyTorch/TensorFlow. Lightweight and embeddable.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 💾 Architecture

Layered cargo workspace, trait-decoupled with a single dependency direction:

```
cli(aria-memo) → memo(orchestration) → storage(SQLite) / embed(local embedding) → core(models/errors/traits)
```

`memo` depends on `core` + `storage` + `embed`; `storage` / `embed` depend only on `core`. There are no upward imports.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔍 Search & retrieval

Hybrid search blends semantic (cosine over local embeddings) and lexical (keyword) relevance:
`score = semantic_weight·cosine + keyword_weight·lexical_relevance`.

- **Lexical pushdown (FTS5).** `memories.content` is indexed with a SQLite FTS5 virtual table (`memories_fts`). On a hybrid query the store runs `MATCH` + `bm25()` *inside SQLite* to prune the candidate set to the top lexical hits first, then computes cosine only on that small set — eliminating the old full-table scan. Pure-semantic queries (or when FTS5 finds no lexical hit) fall back to a full scan so recall is preserved. **Multilingual word segmentation:** because the bundled SQLite build cannot enable the built-in `icu` tokenizer, content is pre-segmented in Rust before FTS5 indexing and the same `segment` logic tokenizes the query, so sub-words match instead of the whole run being one unmatchable token. Routing is by Unicode script: Chinese (zh/zh-TW) via `jieba-rs`, Japanese (ja) and Korean (ko) via `lindera` (embedded ipadic/ko-dic dictionaries), and the remaining space-delimited languages (en/es/fr/de/it/ru/pt/ar/hi) via whitespace — covering all 13 cockpit locales. The FTS index is rebuilt once on open (via `PRAGMA user_version`) so pre-existing databases pick up the segmented tokens (bumped to 4 for the multilingual router).
- **Higher lexical weight.** `SearchQuery.keyword_weight` defaults to `0.5` (was `0.3`), so lexical hits prune and rank earlier; `semantic_weight` stays `0.7`. `MemoryController` uses the same `0.7 / 0.5` blend.
- **Batch retrieval.** `MemoStore::search_batch` / `MemoManager::search_batch` / `recall_batch` score many queries under a single connection lock (one FTS5 prep + one scan), returning `Vec<Vec<ScoredMemo>>`. The CLI exposes `aria-memo search-batch --text … --text …` and `aria-memo bench --batch` (which reports `batch.ops_per_sec`).

**Offline retrieval quality.** The flat `search` path is a hybrid of a local semantic score (cosine over the hashing embedder) and a dependency-free lexical score (`lexical_relevance`: rarity-weighted term overlap plus an exact-phrase bonus, with CJK character 2-grams). Because the bundled embedder is a lightweight hash/TF-IDF vectorizer — no heavy ML — the semantic signal is intentionally weak; `lexical_relevance` maximizes the keyword contribution so offline recall stays useful. Tune `SearchQuery.keyword_weight` (default `0.5`) to shift the balance.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🚀 Quick start

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "User likes Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

`cargo test` passes without Postgres, network, or credentials — everything is local SQLite.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🧰 Command line

`aria-memo` exposes the full memory lifecycle plus two self-management commands (mirroring `aria-router`):

| Command | Key flags | Purpose |
|---|---|---|
| `add` | `--type` `--content` `--importance` (0.5) | Write a memory |
| `get` | `--id` | Read one memory |
| `search` | `--text` `--top-k` (5) `--json` `--graph` | Hybrid search (optionally graph-aware) |
| `recall` | `--text` `--top-k` (5) | Pure-vector recall |
| `search-batch` | `--text` (repeatable) | Batch hybrid search under one lock |
| `list` | `--json` | List memories |
| `update` | `--id` `--content` | Update a memory (operation-level hooks for HaluMem) |
| `forget` | `--id` | Forget a memory |
| `bench` | `--size` `--batch` `--json` | Throughput benchmark |
| `connect` | `--id` | Infer four-view edges for a stored memory |
| `relate` | `--from` `--to` `--kind` `--score` | Manually add a relation edge |
| `relations` | `--from` `--to` `--kind` `--json` | List relation edges |
| `graph` | `--text` `--views` `--budget` `--max-hops` `--top-k` `--json` | Bounded multi-relational traversal |
| `setup` | `--status` `--clear` | Persist CLI config (`~/.ariacompute/memo-cli.yml`) |
| `upgrade` | `[version]` `--url` | Replace binary from GitHub/Gitee Releases |
| `--version` / `-v` | — | Print build version |

**Self-management.** `aria-memo setup` persists the CLI config to `~/.ariacompute/memo-cli.yml` (the `upgrade_url` Releases org root). Interactive runs prompt you to pick **GitHub** (`https://github.com/ariacompute`, default) or **Gitee** (`https://gitee.com/ariacompute`). Non-interactive runs honor `ARIA_MEMO_SITE=cn` (→ Gitee), fall back to any existing config, then to GitHub. `setup --status` prints the config path and `upgrade_url`; `setup --clear` deletes the config file.

`aria-memo upgrade [version]` replaces the running binary with the latest stable release (or a specific `version` tag) from GitHub/Gitee Releases. `upgrade --url <URL>` overrides `upgrade_url` for this run. The resolved `upgrade_url` precedence is: `--url` > `~/.ariacompute/memo-cli.yml` > `ARIA_MEMO_UPGRADE_URL` env > built-in default (`https://github.com/ariacompute`). Release assets follow `aria-memo_{ver}_{os}.tar.gz` (`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`; `.zip` on Windows).

> The home directory `~/.ariacompute` is overridable via `ARIA_COMPUTE_HOME`.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🕸️ Multi-relational memory plane

A structured, multi-relational memory layer on top of the flat `MemoStore` — zero network dependency, offline by default (an LLM-backed `RelationScorer` is pluggable).

Four relation views connect memories with directed edges:

- `semantic` — meaning association (cosine over embeddings)
- `temporal` — ordering / co-occurrence
- `causal` — causal chain
- `entity` — same-entity aggregation

Edges live in a separate `relations` table and never mutate `Memo`. On write, `connect` infers four-view edges; on read, `retrieve` does hybrid search to seed anchors, then a bounded graph expansion across views, returning scored memories plus an inspectable `RetrieveTrace` (Jev-Mem's Retrieve→Assess→Expand, transparent).

```bash
# Infer four-view edges for an already-stored memory
cargo run -p aria-memo -- connect --id <id>

# Manually add an edge
cargo run -p aria-memo -- relate --from <id> --to <id> --kind semantic --score 0.8

# List edges (filter by from / to / kind)
cargo run -p aria-memo -- relations --kind semantic --json

# Bounded multi-relational traversal (views/budget/max-hops/top-k)
cargo run -p aria-memo -- graph --text "Rust systems" --views semantic,entity --top-k 20 --json

# Or turn graph awareness on for plain search
cargo run -p aria-memo -- search --text "Rust" --graph
```

The default `LocalRelationScorer` is fully local (semantic = cosine, temporal = time order, entity = token Jaccard, causal = time-adjacency + overlap). Swap in an LLM-backed scorer without touching the controller.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📊 Benchmarks & comparison

Compare against: mem0 / MemOS / MemPalace / Zep / Letta, plus local embedded **control groups** `sqlite_vec` and `chromem` (included by default; skipped with a recorded `reason` when their dependency/binary is missing — never faked).

- Feature matrix: [docs/compare.md](./docs/compare.md)
- Results guide: [docs/bench_results.md](./docs/bench_results.md)
- Python harness (Track A storage/retrieval + Track B end-to-end quality): [benches/README.md](./benches/README.md)

```bash
pip install -r benches/requirements.txt

pushd benches/chromem-wrapper
go get github.com/philippgille/chromem-go@latest && go mod tidy && go build -o chromem .
export CHROMEM_BIN="$PWD/chromem"
popd

python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem
python benches/run.py --track b --download
```

### Track A evaluation report

Run with `python benches/run.py --track a --sizes 1000,10000,100000 --systems aria,sqlite_vec,chromem` (results under `benches/results/`). This report's figures:
- aria: `python benches/run.py --track a --sizes 1000,10000,100000 --systems aria`
- sqlite_vec: `python benches/run.py --track a --sizes 1000,10000,100000 --systems sqlite_vec`
- chromem: `python benches/run.py --track a --sizes 1000,10000,100000 --systems chromem`

**A1 — Microbench (top_k=5, offline; aria `add_baseline` = per-item embed + per-item txn):**

| System | Size | Segment | ops/sec | p50 (ms) | p99 (ms) | Note |
|--------|------|---------|--------:|---------:|---------:|------|
| aria | 1000 | `add_baseline` | 157.18 | 5.91 | 13.35 | naive per-item path (baseline) |
| aria | 1000 | `search` | 1456.11 | 0.69 | 0.69 | per-query amortized |
| aria | 10000 | `add_wal` | 752.06 | 1.06 | 4.08 | WAL journal mode |
| aria | 10000 | `add_batch_embed` | 40039.03 | 0.02 | 0.02 | batch embed + transactional write |
| aria | 10000 | `add_bulk` | 72164.68 | 0.01 | 0.01 | per-item embed, txn merge |
| aria | 10000 | `search` | 140.71 | 7.11 | 7.11 | per-query amortized |
| aria | 100000 | `add_batch_embed` | 41459.87 | 0.02 | 0.02 | only optimized segment at 100k |
| aria | 100000 | `search` | 13.65 | 73.25 | 73.25 | per-query amortized (full-corpus scan) |
| sqlite_vec | 1000 | `add` | 23345.52 | 0.04 | 0.04 | batched insert |
| sqlite_vec | 1000 | `search` | 592.15 | 1.69 | 1.69 | per-query amortized |
| sqlite_vec | 10000 | `add` | 22223.95 | 0.04 | 0.04 | batched insert |
| sqlite_vec | 10000 | `search` | 50.42 | 19.84 | 19.84 | per-query amortized |
| sqlite_vec | 100000 | `add` | 20212.36 | 0.05 | 0.05 | batched insert |
| sqlite_vec | 100000 | `search` | 4.58 | 218.25 | 218.25 | per-query amortized (full-corpus scan) |
| chromem | 1000 | `add` | 22126.47 | 0.05 | 0.05 | batched insert (`add-batch`) |
| chromem | 1000 | `search` | 208.95 | 4.79 | 4.79 | per-query amortized (`query-batch`) |
| chromem | 10000 | `add` | 23672.96 | 0.04 | 0.04 | batched insert (`add-batch`) |
| chromem | 10000 | `search` | 23.08 | 43.34 | 43.34 | per-query amortized (`query-batch`) |
| chromem | 100000 | `add` | 23089.77 | 0.04 | 0.04 | batched insert (`add-batch`) |
| chromem | 100000 | `search` | 2.33 | 429.42 | 429.42 | per-query amortized (`query-batch`) |

**A2 — Retrieval quality (synthetic_v2, 84 queries, top_k=5):**

| System | Recall@5 | MRR | Queries | Offline |
|--------|---------:|----:|--------:|:-------:|
| aria | 1.00 | 1.00 | 84 | true |
| sqlite_vec | 0.9762 | 0.9762 | 84 | true |
| chromem | 1.00 | 1.00 | 84 | true |

### Track B evaluation report

End-to-end memory quality across four benchmarks: `locomo_refined`, `halumem`, `longmemeval`, `personamem`. The aria backend is driven via the CLI (build first: `cargo build -p aria-memo --release`).

```bash
# optional: verify dataset loading + backend capabilities before scoring
python benches/run.py --track b --dry-run

# real datasets (auto-download; fails loudly with manual hints if network blocked)
python benches/run.py --track b --download
```

Offline metrics (F1/BLEU, multiple-choice, retrieval Recall@k, extraction recall) run with **no LLM**. LLM-judge metrics are **optional** — skipped silently when no credentials are present:

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://tokenhub.tencentmaas.com
python benches/run.py --track b --download --judge-model hy3
```

> Each benchmark falls back to the bundled `benches/data/fixtures/` (synthetic, offline smoke) when real data is absent; `dataset_source` is recorded in the report. Use `--benchmarks`, `--limit`, or `--ingest-only` to scope a run.

**Scoping large real datasets.** Real datasets can be huge — e.g. `halumem` (HaluMem-Medium) holds ~75k `add` calls. `--limit N` bounds how many samples are *ingested and evaluated* per benchmark (for `halumem` each record is one sample set; for `locomo_refined`/`longmemeval`/`personamem` it caps questions/items). When `--limit` is omitted, a default cap of `50` samples/benchmark is applied (printed to stderr) so an unbounded run does not hang. Ingestion prints progress to stderr (`[bench] ingest 500/N ... ingest done`).

```bash
# bounded, fast smoke run
python benches/run.py --track b --limit 2
# full-scale (long but bounded); build a release binary to speed up each add
cargo build -p aria-memo --release
python benches/run.py --track b --benchmarks halumem
```

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📂 Directory

- `crates/core` — data models, unified `MemoError`, traits
- `crates/embed` — lightweight local embedder (ngram + hash/TF-IDF vectors) + cosine similarity
- `crates/storage` — rusqlite bundled embedded persistence backend
- `crates/memo` — memory management orchestration & lifecycle; multi-relational `MemoryController` / `RelationScorer`
- `crates/cli` — command-line entry point
- `benches/` — industry comparison harness
- `docs/` — feature matrix and benchmark notes

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔏 Trust store security

The 2026-10 eMudhra (emSign) intermediate CA key compromise is covered in three layers:

- **Rust CLI (`aria-memo`).** Outbound HTTPS (the self-managed `upgrade --url` release download and any future HTTP fetch) goes through `reqwest` + `rustls-tls`, whose trust store is the compiled-in **`webpki-roots`**, pinned `>=1.0.10` in `crates/cli/Cargo.toml` and bumped in `Cargo.lock` (1.0.9 → 1.0.10) — 1.0.10+ removes the compromised emSign roots. The binary does **not** use the system bundle, so the lock bump is the decisive fix.
- **Python benchmarks (`benches/`).** The harness may make outbound TLS (datasets download, LLM judge API); `benches/requirements.txt` pins `certifi` so its bundled CA store is a post-emSign-removal release.
- **Offline detector.** `scripts/check-emudhra.sh` scans a host/system CA bundle for the known emSign root fingerprints:

```bash
scripts/check-emudhra.sh /etc/ssl/certs/ca-certificates.crt
```

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🔨 Development

| Target | Effect |
|---|---|
| `cargo test` | `cargo test --workspace` |
| `cargo test -p memo-core` | unit tests for the core crate |
| `cargo build` | build all crates |
| `cargo run -p aria-memo -- <cmd>` | run the CLI |
| `cargo clippy --all-targets` | lint all targets |
| `cargo fmt --all -- --check` | verify formatting |
| `scripts/check-emudhra.sh <bundle>` | offline eMudhra/emSign CA fingerprint scan |

`cargo test` passes without Postgres, network, or credentials — every external dependency is local (SQLite) or faked.

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📚 Documentation

All detailed docs live under [`docs/`](./docs):

| Document | What it covers |
|---|---|
| [Feature matrix](./docs/compare.md) | memo vs mem0 / MemOS / MemPalace / Zep / Letta + local control groups |
| [Benchmark results](./docs/bench_results.md) | Track A/B methodology and results |
| [Benchmark harness](./benches/README.md) | Python orchestration (Track A storage/retrieval + Track B end-to-end) |
| [Requirements](./requirements.md) | feature boundaries / exceptions / acceptance criteria |
| [Task list](./task.md) | implementation checklist |
| [AGENTS.md](./AGENTS.md) | agent engineering context entry and directory index |

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 📝 License

Apache-2.0. See [LICENSE](./LICENSE).

<div align="right">

[![][back-to-top]](#readme-top)

</div>

## 🤝 Engineering conventions

This repository follows the Harness Engineering philosophy: the repository is the agent's context, and the documents that govern it are versioned alongside the code.

- [`AGENTS.md`](./AGENTS.md) — agent engineering context entry and directory index
- [`requirements.md`](./requirements.md) — requirements spec (feature boundaries / exceptions / acceptance criteria), human-review-gated before implementation
- [`task.md`](./task.md) — implementation task checklist

<div align="right">

[![][back-to-top]](#readme-top)

</div>

---

<div align="center">

Copyright © 2026 [ariacompute][org]. <br/>
This project is [Apache-2.0](./LICENSE) licensed.

</div>

<!-- LINK GROUP -->

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
