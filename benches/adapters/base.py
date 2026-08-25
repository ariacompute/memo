from __future__ import annotations

"""Unified memory-backend contract: both Track A and Track B call systems through this interface."""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


class UnsupportedCapability(RuntimeError):
    """Raised when an optional capability not implemented by the backend is requested."""


@dataclass
class SearchHit:
    id: str
    content: str
    score: float
    metadata: dict[str, Any] = field(default_factory=dict)


@dataclass
class BackendInfo:
    name: str
    available: bool
    reason: str = ""
    includes_network: bool = False
    offline: bool = True


class MemoBackend(ABC):
    """Minimal contract: add / search / reset; optional capabilities degrade by default."""

    @abstractmethod
    def info(self) -> BackendInfo:
        ...

    @abstractmethod
    def reset(self) -> None:
        ...

    @abstractmethod
    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        ...

    @abstractmethod
    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        ...

    # ---------- convenience methods ----------
    def name(self) -> str:
        return self.info().name

    # ---------- optional capabilities (degrade by default) ----------
    def supports(self, cap: str) -> bool:
        """cap: 'list_memories' | 'update'. Both unsupported by default."""
        return False

    def list_memories(self) -> list[SearchHit]:
        raise UnsupportedCapability("list_memories")

    def update(self, memo_id: str, content: str | None = None, importance: float | None = None) -> None:
        raise UnsupportedCapability("update")

    def close(self) -> None:
        return None
