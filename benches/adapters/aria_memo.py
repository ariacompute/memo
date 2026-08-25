from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
from pathlib import Path
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


def _find_bin() -> str | None:
    env = os.environ.get("ARIA_MEMO_BIN")
    if env and Path(env).is_file():
        return env
    root = Path(__file__).resolve().parents[2]
    for cand in (
        root / "target" / "release" / "aria-memo",
        root / "target" / "debug" / "aria-memo",
    ):
        if cand.is_file():
            return str(cand)
    which = shutil.which("aria-memo")
    return which


class AriaMemoBackend(MemoBackend):
    """Drive aria-memo via CLI (local SQLite, zero network)."""

    def __init__(self, bin_path: str | None = None) -> None:
        self._bin = bin_path or _find_bin()
        self._db: str | None = None
        self._tmpdir: tempfile.TemporaryDirectory[str] | None = None

    def info(self) -> BackendInfo:
        if not self._bin:
            return BackendInfo(
                name="aria",
                available=False,
                reason="aria-memo binary not found; build with `cargo build -p aria-memo --release`",
                includes_network=False,
                offline=True,
            )
        return BackendInfo(
            name="aria",
            available=True,
            includes_network=False,
            offline=True,
        )

    def reset(self) -> None:
        self.close()
        self._tmpdir = tempfile.TemporaryDirectory(prefix="aria-bench-")
        self._db = str(Path(self._tmpdir.name) / "memo.db")

    def _ensure(self) -> None:
        if self._db is None:
            self.reset()
        if not self._bin:
            raise RuntimeError("aria-memo binary missing")

    def _run(self, *args: str) -> str:
        self._ensure()
        cmd = [self._bin, "--db", self._db or "memo.db", *args]
        proc = subprocess.run(cmd, capture_output=True, text=True, check=False)
        if proc.returncode != 0:
            raise RuntimeError(proc.stderr.strip() or proc.stdout.strip() or "cli failed")
        return proc.stdout.strip()

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        meta = metadata or {}
        mtype = str(meta.get("memo_type") or meta.get("type") or "working")
        importance = str(meta.get("importance", 0.5))
        # use the `--content=...` form to avoid content starting with '-' being misparsed as an option by clap
        return self._run(
            "add",
            "--type",
            mtype,
            f"--content={content}",
            "--importance",
            importance,
        )

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        out = self._run("search", f"--text={query}", "--top-k", str(top_k))
        hits: list[SearchHit] = []
        if not out:
            return hits
        for i, line in enumerate(out.splitlines()):
            if "\t" not in line:
                continue
            score_s, content = line.split("\t", 1)
            try:
                score = float(score_s)
            except ValueError:
                score = 0.0
            hits.append(SearchHit(id=str(i), content=content, score=score))
        return hits

    # ---------- optional capabilities ----------
    def supports(self, cap: str) -> bool:
        return cap in ("list_memories", "update")

    def list_memories(self) -> list[SearchHit]:
        out = self._run("list", "--json")
        hits: list[SearchHit] = []
        try:
            arr = json.loads(out)
        except json.JSONDecodeError:
            return hits
        for item in arr:
            hits.append(
                SearchHit(
                    id=str(item.get("id", "")),
                    content=item.get("content", ""),
                    score=float(item.get("importance", 0.0)),
                    metadata={
                        "memo_type": item.get("memo_type"),
                        "version": item.get("version"),
                    },
                )
            )
        return hits

    def update(self, memo_id: str, content: str | None = None, importance: float | None = None) -> None:
        args: list[str] = ["update", "--id", memo_id]
        if content is not None:
            args += [f"--content={content}"]
        if importance is not None:
            args += ["--importance", str(importance)]
        self._run(*args)


    def microbench_json(self, size: int, top_k: int = 5, warmup: int = 10) -> dict[str, Any]:
        """Invoke the in-process `bench`; the hot path excludes CLI startup amortized over each add."""
        info = self.info()
        if not info.available:
            return {"system": "aria", "skipped": True, "reason": info.reason}
        assert self._bin
        proc = subprocess.run(
            [
                self._bin,
                "bench",
                "--size",
                str(size),
                "--top-k",
                str(top_k),
                "--warmup",
                str(warmup),
                "--json",
            ],
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            return {
                "system": "aria",
                "skipped": True,
                "reason": proc.stderr.strip() or "bench failed",
            }
        return json.loads(proc.stdout.strip())

    def close(self) -> None:
        if self._tmpdir is not None:
            self._tmpdir.cleanup()
            self._tmpdir = None
            self._db = None
