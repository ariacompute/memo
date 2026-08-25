from __future__ import annotations

"""统一记忆后端契约：Track A/B 均经此接口调用各系统。"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


class UnsupportedCapability(RuntimeError):
    """请求后端未实现的可选能力时抛出。"""


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
    """最小契约：add / search / reset；可选能力默认降级。"""

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

    # ---------- 便捷方法 ----------
    def name(self) -> str:
        return self.info().name

    # ---------- 可选能力（默认降级） ----------
    def supports(self, cap: str) -> bool:
        """cap: 'list_memories' | 'update'。默认均不支持。"""
        return False

    def list_memories(self) -> list[SearchHit]:
        raise UnsupportedCapability("list_memories")

    def update(self, memo_id: str, content: str | None = None, importance: float | None = None) -> None:
        raise UnsupportedCapability("update")

    def close(self) -> None:
        return None
