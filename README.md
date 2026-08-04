# aria memory

> Docs: [中文](./README_cn.md) ｜ [English](./README.md)

Local-first long-term memory store for LLM Agents, built in Rust for edge/mobile deployment.
Provides CRUD, semantic + keyword hybrid retrieval, consolidation, deduplication, and forgetting —
with zero network dependency and zero heavy ML frameworks.

Inspired by: rqlite / turso (embedded persistence), MemOS / mem0 / MemPalace (memory management).

## Architecture

Layered cargo workspace (trait-decoupled):

```
cli(aria-memory) → memory(orchestration) → storage(SQLite) / embed(local embedding) → core(models/errors/traits)
```

## Quick Start

```bash
cargo build
cargo test
cargo run -p aria-memory -- add --type working --content "User likes Rust" --importance 0.8
cargo run -p aria-memory -- search --text "Rust" --top-k 5
```

## Benchmarks & Comparison

Compare against: mem0 / MemOS / MemPalace / Zep / Letta.

- Feature matrix: [docs/compare.md](./docs/compare.md)
- Results guide: [docs/bench_results.md](./docs/bench_results.md)
- Python harness (Track A storage/retrieval + Track B end-to-end quality): [benches/README.md](./benches/README.md)

```bash
cargo run -p aria-memory -- bench --size 1000 --json
pip install -r benches/requirements.txt
python benches/run.py --track a --size 1000
python benches/run.py --track b --dry-run
```

## Directory

- `crates/core` — data models, unified `MemoryError`, traits
- `crates/embed` — lightweight local embedder (ngram + hash/TF-IDF vectors) + cosine similarity
- `crates/storage` — rusqlite bundled embedded persistence backend
- `crates/memory` — memory management orchestration & lifecycle
- `crates/cli` — command-line entry point
- `benches/` — industry comparison harness
- `docs/` — feature matrix and benchmark notes

## Engineering Conventions

This repository follows the Harness Engineering philosophy:

- [`AGENTS.md`](AGENTS.md): Agent engineering context entry and directory index
- [`requirements.md`](requirements.md): Requirements spec (feature boundaries/exceptions/acceptance criteria, human-review-gated)
- [`task.md`](task.md): Implementation task checklist
