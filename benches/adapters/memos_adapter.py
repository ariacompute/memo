from __future__ import annotations

from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit
from .skip import SkipBackend


class MemosBackend(MemoBackend):
    def __init__(self) -> None:
        self._impl: MemoBackend
        try:
            import memos  # type: ignore  # noqa: F401

            self._impl = SkipBackend(
                "memos",
                "MemOS SDK detected but unified add/search shim not configured; "
                "set MEMOS_ENDPOINT or wire OmniMemEval adapter",
                includes_network=True,
            )
        except Exception:
            self._impl = SkipBackend(
                "memos",
                "MemOS not installed; see https://github.com/MemTensor/MemOS",
            )

    def info(self) -> BackendInfo:
        return self._impl.info()

    def reset(self) -> None:
        self._impl.reset()

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        return self._impl.add(content, metadata)

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        return self._impl.search(query, top_k)
