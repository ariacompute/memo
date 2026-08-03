from __future__ import annotations

from typing import Any

from .base import BackendInfo, MemoryBackend, SearchHit


class SkipBackend(MemoryBackend):
    """不可用时的占位后端：info 标明 skipped，调用 add/search 抛错。"""

    def __init__(self, name: str, reason: str, includes_network: bool = True) -> None:
        self._name = name
        self._reason = reason
        self._includes_network = includes_network

    def info(self) -> BackendInfo:
        return BackendInfo(
            name=self._name,
            available=False,
            reason=self._reason,
            includes_network=self._includes_network,
            offline=False,
        )

    def reset(self) -> None:
        return None

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        raise RuntimeError(f"{self._name} unavailable: {self._reason}")

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        raise RuntimeError(f"{self._name} unavailable: {self._reason}")
