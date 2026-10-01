from __future__ import annotations

"""Local-embedded control group: chromem-go driven as a subprocess.

chromem is a Go library with no mature Python binding, so we drive a `chromem` CLI binary
via subprocess (same shape as the aria-memo adapter). The assumed CLI surface is:

    chromem <db> add   --content <text> [--metadata <json>]
    chromem <db> query --text <text> --top-k <k>     (prints `score\\tcontent` lines)

Embeddings are produced by chromem itself (configure its offline embedder when starting the
binary). This backend is **local, offline**. When the binary is absent the backend reports
`available=False` and the harness skips it (writing the reason).

NOTE: the exact CLI flags depend on the chromem-go build you provide via `CHROMEM_BIN`;
adjust the `_run` argument assembly if your binary differs. The adapter only needs to be
importable and skip gracefully when the binary is missing.
"""

import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


def _find_bin() -> str | None:
    env = os.environ.get("CHROMEM_BIN")
    if env and Path(env).is_file():
        return env
    return shutil.which("chromem")


class ChromemBackend(MemoBackend):
    """Drive a `chromem` CLI binary through subprocess calls."""

    def __init__(self, bin_path: str | None = None, timeout_s: float | None = None) -> None:
        self._bin = bin_path or _find_bin()
        self._timeout_s = (
            timeout_s if timeout_s is not None else float(os.environ.get("CHROMEM_TIMEOUT", "120"))
        )
        self._db: str | None = None
        self._tmp: tempfile.TemporaryDirectory[str] | None = None

    def info(self) -> BackendInfo:
        if not self._bin:
            return BackendInfo(
                name="chromem",
                available=False,
                reason="chromem-go binary not found (set CHROMEM_BIN or put `chromem` on PATH)",
                includes_network=False,
                offline=True,
            )
        return BackendInfo(
            name="chromem",
            available=True,
            includes_network=False,
            offline=True,
        )

    def reset(self) -> None:
        self.close()
        self._tmp = tempfile.TemporaryDirectory(prefix="chromem-bench-")
        self._db = str(Path(self._tmp.name) / "chromem.db")

    def _ensure(self) -> None:
        if self._db is None:
            self.reset()
        if not self._bin:
            raise RuntimeError("chromem binary missing")

    def _run(self, *args: str) -> str:
        self._ensure()
        assert self._bin and self._db
        cmd = [self._bin, self._db, *args]
        try:
            proc = subprocess.run(
                cmd, capture_output=True, text=True, check=False, timeout=self._timeout_s
            )
        except subprocess.TimeoutExpired:
            raise RuntimeError(
                f"chromem timed out after {self._timeout_s}s: {' '.join(cmd)}"
            ) from None
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or "chromem failed")
        return proc.stdout.strip()

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        # use --content= form to avoid content starting with '-' being parsed as a flag
        return self._run("add", f"--content={content}")

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        out = self._run("query", f"--text={query}", "--top-k", str(top_k))
        hits: list[SearchHit] = []
        if not out:
            return hits
        for line in out.splitlines():
            if "\t" not in line:
                continue
            score_s, content = line.split("\t", 1)
            try:
                score = float(score_s)
            except ValueError:
                score = 0.0
            hits.append(SearchHit(id=str(len(hits)), content=content, score=score))
        return hits

    def close(self) -> None:
        if self._tmp is not None:
            self._tmp.cleanup()
            self._tmp = None
            self._db = None
