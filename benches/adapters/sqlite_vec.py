from __future__ import annotations

"""Local-embedded control group: sqlite-vec (pip `sqlite-vec`) + an offline hashed-n-gram embedder.

This mirrors aria's `LocalEmbedder` (hashing trick + L2 normalization) so the comparison
matrix measures the storage/vector-index layer fairly, not the embedding quality. It is a
**local, offline** backend: no network, no LLM. When `sqlite-vec` is not installed the
backend reports `available=False` and the harness skips it (writing the reason), per the
project's "missing dependency -> skip, never fabricate numbers" rule.
"""

import sqlite3
import struct
import tempfile
from pathlib import Path
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit

DEFAULT_DIM = 64


def _embed(text: str, dim: int = DEFAULT_DIM) -> list[float]:
    """Deterministic offline embedding (same shape as aria's LocalEmbedder)."""
    lower = text.lower()
    words = [w for w in lower.split() if w]
    toks: list[str] = list(words)
    for a, b in zip(words, words[1:]):
        toks.append(f"{a} {b}")
    chars = [c for c in lower if c.isalnum()]
    for a, b in zip(chars, chars[1:]):
        toks.append(a + b)
    if not toks:
        return [0.0] * dim
    counts: dict[int, float] = {}
    for t in toks:
        h = 0
        for ch in t.encode("utf-8"):
            h = (h ^ ch) * 0x100000001B3 & 0xFFFFFFFFFFFFFFFF
        counts[h % dim] = counts.get(h % dim, 0.0) + 1.0
    vec = [0.0] * dim
    mx = max(counts.values())
    for i, c in counts.items():
        vec[i] = (c / mx) ** 0.5
    norm = sum(v * v for v in vec) ** 0.5
    if norm == 0.0:
        return [0.0] * dim
    return [v / norm for v in vec]


def _to_blob(vec: list[float]) -> bytes:
    """Pack a float vector as a little-endian float32 blob for sqlite-vec.

    sqlite-vec's `vec0` virtual table binds embeddings as a packed `bytes` object
    (not a Python `list`, which Python's sqlite3 driver refuses to bind — that
    produced the earlier `Error binding parameter 2: type 'list' is not supported`).
    """
    return struct.pack(f"<{len(vec)}f", *vec)


class SqliteVecBackend(MemoBackend):
    """Drive a local SQLite database with the `vec0` vector index from `sqlite-vec`."""

    def __init__(self, db_path: str | None = None, dim: int = DEFAULT_DIM) -> None:
        self._dim = dim
        self._db_path = db_path
        self._tmp: tempfile.TemporaryDirectory[str] | None = None
        self._conn: sqlite3.Connection | None = None
        self._next_id = 0

    def info(self) -> BackendInfo:
        try:
            import sqlite_vec  # noqa: F401
        except Exception as e:  # noqa: BLE001
            return BackendInfo(
                name="sqlite_vec",
                available=False,
                reason=f"sqlite-vec not installed: {e}; pip install sqlite-vec",
                includes_network=False,
                offline=True,
            )
        return BackendInfo(
            name="sqlite_vec",
            available=True,
            includes_network=False,
            offline=True,
        )

    def reset(self) -> None:
        self.close()
        if self._db_path is None:
            self._tmp = tempfile.TemporaryDirectory(prefix="sqlite-vec-bench-")
            self._db_path = str(Path(self._tmp.name) / "memo.db")
        self._conn = sqlite3.connect(self._db_path)
        try:
            import sqlite_vec

            self._conn.enable_load_extension(True)
            sqlite_vec.load(self._conn)
        except Exception as e:  # noqa: BLE001
            raise RuntimeError(f"failed to load sqlite-vec extension: {e}") from e
        self._conn.execute(
            "CREATE TABLE IF NOT EXISTS memos (id INTEGER PRIMARY KEY, content TEXT, meta TEXT)"
        )
        self._conn.execute(
            f"CREATE VIRTUAL TABLE IF NOT EXISTS memo_vec USING vec0(embedding float[{self._dim}])"
        )
        # Speed: WAL + synchronous=NORMAL so large corpora don't fsync on every
        # commit. Without this, add()'s per-insert commit() does 100k fsyncs at
        # size=100000 and the microbench appears to hang. Per-add timing is
        # preserved (commit() still runs), just without the disk flush cost.
        self._conn.execute("PRAGMA journal_mode=WAL")
        self._conn.execute("PRAGMA synchronous=NORMAL")
        self._conn.commit()
        self._next_id = 0

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        if self._conn is None:
            self.reset()
        assert self._conn is not None
        self._next_id += 1
        mid = self._next_id
        vec = _embed(content, self._dim)
        self._conn.execute(
            "INSERT INTO memos (id, content, meta) VALUES (?, ?, ?)", (mid, content, "")
        )
        self._conn.execute(
            "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)", (mid, _to_blob(vec))
        )
        self._conn.commit()
        return str(mid)

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        if self._conn is None:
            self.reset()
        assert self._conn is not None
        qv = _embed(query, self._dim)
        rows = self._conn.execute(
            "SELECT m.id, m.content, vec_distance_L2(v.embedding, ?) AS d "
            "FROM memo_vec v JOIN memos m ON m.id = v.rowid "
            "ORDER BY d LIMIT ?",
            (_to_blob(qv), top_k),
        ).fetchall()
        hits: list[SearchHit] = []
        for mid, content, d in rows:
            # L2 distance -> bounded similarity score for report compatibility.
            score = 1.0 / (1.0 + float(d))
            hits.append(SearchHit(id=str(mid), content=content, score=score))
        return hits

    def search_batch(self, queries: list[str], top_k: int = 5) -> list[list[SearchHit]]:
        """Batched query: serve every query in one in-process call (no per-query round trip).

        The harness amortizes the per-query latency across `size` samples, which avoids the
        O(n^2) cost of looping `search()` `size` times at large corpora (vec0 does a full
        linear scan per query, so 100k×100k would take tens of minutes and appear to hang).
        """
        if self._conn is None:
            self.reset()
        assert self._conn is not None
        results: list[list[SearchHit]] = []
        for q in queries:
            qv = _embed(q, self._dim)
            rows = self._conn.execute(
                "SELECT m.id, m.content, vec_distance_L2(v.embedding, ?) AS d "
                "FROM memo_vec v JOIN memos m ON m.id = v.rowid "
                "ORDER BY d LIMIT ?",
                (_to_blob(qv), top_k),
            ).fetchall()
            hits: list[SearchHit] = []
            for mid, content, d in rows:
                score = 1.0 / (1.0 + float(d))
                hits.append(SearchHit(id=str(mid), content=content, score=score))
            results.append(hits)
        return results

    def add_batch(self, contents: list[str]) -> int:
        """Batched insert: one transactional write for the whole corpus (mirrors chromem's
        `add-batch`). Avoids 100k per-item commits and keeps the 100k microbench fast.
        """
        if self._conn is None:
            self.reset()
        assert self._conn is not None
        memo_rows: list[tuple[int, str, str]] = []
        vec_rows: list[tuple[int, bytes]] = []
        for c in contents:
            self._next_id += 1
            mid = self._next_id
            memo_rows.append((mid, c, ""))
            vec_rows.append((mid, _to_blob(_embed(c, self._dim))))
        self._conn.executemany(
            "INSERT INTO memos (id, content, meta) VALUES (?, ?, ?)", memo_rows
        )
        self._conn.executemany(
            "INSERT INTO memo_vec(rowid, embedding) VALUES (?, ?)", vec_rows
        )
        self._conn.commit()
        return len(contents)

    def close(self) -> None:
        if self._conn is not None:
            self._conn.close()
            self._conn = None
        if self._tmp is not None:
            self._tmp.cleanup()
            self._tmp = None
            self._db_path = None
