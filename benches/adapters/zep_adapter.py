from __future__ import annotations

import os
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


class ZepBackend(MemoBackend):
    def __init__(self) -> None:
        self._client: Any = None
        self._reason = ""
        key = os.environ.get("ZEP_API_KEY")
        if not key:
            self._reason = "ZEP_API_KEY not set"
            return
        try:
            from zep_cloud.client import Zep  # type: ignore

            self._client = Zep(api_key=key)
        except Exception as e:  # noqa: BLE001
            self._reason = f"zep SDK unavailable ({e}); pip install zep-cloud"

    def info(self) -> BackendInfo:
        ok = self._client is not None
        return BackendInfo(
            name="zep",
            available=ok,
            reason="" if ok else self._reason,
            includes_network=True,
            offline=False,
        )

    def reset(self) -> None:
        return None

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        _ = metadata
        raise RuntimeError(
            "Zep adapter requires session/graph wiring; extend ZepBackend.add for your project"
        )

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        _ = query, top_k
        raise RuntimeError("Zep adapter search not fully wired")
