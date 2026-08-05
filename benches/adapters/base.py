from __future__ import annotations

"""统一记忆后端契约：Track A/B 均经此接口调用各系统。"""

from abc import ABC, abstractmethod
from dataclasses import dataclass, field
from typing import Any


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
    """最小契约：add / search / reset。"""

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

    def close(self) -> None:
        return None
