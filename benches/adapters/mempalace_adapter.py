from __future__ import annotations

from typing import Any

from .base import BackendInfo, MemoryBackend, SearchHit
from .skip import SkipBackend


class MemPalaceBackend(MemoryBackend):
    def __init__(self) -> None:
        self._impl: MemoryBackend
        try:
            import mempalace  # type: ignore  # noqa: F401

            self._impl = SkipBackend(
                "mempalace",
                "MemPalace import ok but local API shim pending; wire project entrypoint",
                includes_network=False,
            )
        except Exception:
            self._impl = SkipBackend(
                "mempalace",
                "MemPalace not installed; install upstream package to enable",
                includes_network=False,
            )

    def info(self) -> BackendInfo:
        return self._impl.info()

    def reset(self) -> None:
        self._impl.reset()

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        return self._impl.add(content, metadata)

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        return self._impl.search(query, top_k)
