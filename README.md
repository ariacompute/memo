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

## Directory

- `crates/core` — data models, unified `MemoryError`, traits
- `crates/embed` — lightweight local embedder (ngram + hash/TF-IDF vectors) + cosine similarity
- `crates/storage` — rusqlite bundled embedded persistence backend
- `crates/memory` — memory management orchestration & lifecycle
- `crates/cli` — command-line entry point
