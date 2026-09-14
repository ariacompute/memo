# memo

[English](README.md) | [中文](README_cn.md)

Local-first long-term memory store for LLM Agents, built in Rust for edge/mobile deployment.
Provides CRUD, semantic + keyword hybrid retrieval, consolidation, deduplication, and forgetting —
with zero network dependency and zero heavy ML frameworks.

Inspired by: rqlite / turso (embedded persistence), MemOS / mem0 / MemPalace (memo management).

## Architecture

Layered cargo workspace (trait-decoupled):

```
cli(aria-memo) → memo(orchestration) → storage(SQLite) / embed(local embedding) → core(models/errors/traits)
```

## Quick Start

```bash
cargo build
cargo test
cargo run -p aria-memo -- add --type working --content "User likes Rust" --importance 0.8
cargo run -p aria-memo -- search --text "Rust" --top-k 5
```

## CLI & Self-Management

`aria-memo` ships two self-management commands (mirroring `aria-router`):

- `aria-memo setup` — persist the CLI config to `~/.ariacompute/memo-cli.yml`
  (the `upgrade_url` Releases org root). Interactive runs prompt you to pick
  **GitHub** (`https://github.com/ariacompute`, default) or **Gitee**
  (`https://gitee.com/ariacompute`). Non-interactive runs honor
  `ARIA_MEMO_SITE=cn` (→ Gitee), fall back to any existing config, then to GitHub.
  - `aria-memo setup --status` — print the config path and `upgrade_url`.
  - `aria-memo setup --clear` — delete the config file.
- `aria-memo upgrade [version]` — replace the running binary with the latest
  stable release (or a specific `version` tag) from GitHub/Gitee Releases.
  - `aria-memo upgrade --url <URL>` — override `upgrade_url` from config for this run.
  - The resolved `upgrade_url` precedence is:
    `--url` > `~/.ariacompute/memo-cli.yml` > `ARIA_MEMO_UPGRADE_URL` env > built-in
    default (`https://github.com/ariacompute`).
  - Release assets follow `aria-memo_{ver}_{os}.tar.gz`
    (`os` ∈ `linux_x86_64`/`linux_arm64`/`macos`/`windows_x86_64`; `.zip` on Windows).

`aria-memo --version` / `-v` prints the build version.

> The home directory `~/.ariacompute` is overridable via `ARIA_COMPUTE_HOME`.

## Benchmarks & Comparison

Compare against: mem0 / MemOS / MemPalace / Zep / Letta.

- Feature matrix: [docs/compare.md](./docs/compare.md)
- Results guide: [docs/bench_results.md](./docs/bench_results.md)
- Python harness (Track A storage/retrieval + Track B end-to-end quality): [benches/README.md](./benches/README.md)

```bash
cargo run -p aria-memo -- bench --size 1000 --json
pip install -r benches/requirements.txt
python benches/run.py --track a --size 1000
python benches/run.py --track b --dry-run
```

### Benchmark Report

Run locally with `cargo run -p aria-memo -- bench --size 1000 --json`
(local-first, offline, zero network).

| Operation | Dataset size | ops/sec | p50 (ms) | p99 (ms) |
|-----------|-------------:|--------:|---------:|---------:|
| `add`     | 1000         | 161.18  | 5.15     | 17.76    |
| `search`  | 1000         | 79.88   | 11.74    | 25.58    |

> Environment: local-first embedded store (rusqlite bundled), no network, `top_k=5`, warmup=10.
> Numbers are illustrative from a single dev run; rebuild and re-run for your own hardware.

### Track A evaluation report

Run with `python benches/run.py --track a --size 1000`
(results under `benches/results/`, e.g. `20260825T054023Z/track_a.json`).

**A1 — Microbench (aria-memo, size=1000, top_k=5, offline):**

| Operation | ops/sec | p50 (ms) | p99 (ms) |
|-----------|--------:|---------:|---------:|
| `add`     | 177.60  | 4.79     | 18.04    |
| `search`  | 636.91  | 1.47     | 3.00     |

**A2 — Retrieval quality (synthetic_retrieval.json, 8 queries, top_k=5):**

| System | Recall@5 | MRR  | Queries | Offline |
|--------|---------:|-----:|--------:|:-------:|
| aria   | 1.00     | 1.00 | 8       | true    |

> A1 reuses the in-process CLI `bench` JSON; A2 measures hybrid (semantic + keyword) retrieval on a synthetic dataset. See [docs/compare.md](./docs/compare.md) for the full comparison matrix.

### Track B evaluation report

End-to-end memory quality across four benchmarks: `locomo_refined`, `halumem`, `longmemeval`, `personamem`.
The aria backend is driven via the CLI (build first: `cargo build -p aria-memo --release`).

```bash
# optional: verify dataset loading + backend capabilities before scoring
python benches/run.py --track b --dry-run

# real datasets (auto-download; fails loudly with manual hints if network blocked)
python benches/run.py --track b --download
```

Offline metrics (F1/BLEU, multiple-choice, retrieval Recall@k, extraction recall) run with **no LLM**.
LLM-judge metrics are **optional** — skipped silently when no credentials are present:

```bash
export BENCH_LLM_API_KEY=sk-...
export BENCH_LLM_BASE_URL=https://...   # optional, OpenAI-compatible
python benches/run.py --track b --judge-model gpt-4o-mini
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

## Directory

- `crates/core` — data models, unified `MemoError`, traits
- `crates/embed` — lightweight local embedder (ngram + hash/TF-IDF vectors) + cosine similarity
- `crates/storage` — rusqlite bundled embedded persistence backend
- `crates/memo` — memory management orchestration & lifecycle
- `crates/cli` — command-line entry point
- `benches/` — industry comparison harness
- `docs/` — feature matrix and benchmark notes

## Engineering Conventions

This repository follows the Harness Engineering philosophy:

- [`AGENTS.md`](AGENTS.md): Agent engineering context entry and directory index
- [`requirements.md`](requirements.md): Requirements spec (feature boundaries/exceptions/acceptance criteria, human-review-gated)
- [`task.md`](task.md): Implementation task checklist
