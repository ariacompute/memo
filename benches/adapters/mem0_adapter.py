from __future__ import annotations

import os
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


class Mem0Backend(MemoBackend):
    """mem0：优先 MemoryClient（云）；否则尝试 Memo（OSS）。"""

    def __init__(self) -> None:
        self._client: Any = None
        self._user_id = os.environ.get("MEM0_USER_ID", "aria-bench")
        self._mode = "none"
        self._reason = ""
        api_key = os.environ.get("MEM0_API_KEY")
        try:
            if api_key:
                from mem0 import MemoryClient  # type: ignore

                self._client = MemoryClient(api_key=api_key)
                self._mode = "client"
            else:
                from mem0 import Memory  # type: ignore

                self._client = Memory()
                self._mode = "oss"
        except Exception as e:  # noqa: BLE001
            self._reason = (
                f"mem0 not usable ({e}); pip install mem0ai and/or set MEM0_API_KEY"
            )

    def info(self) -> BackendInfo:
        ok = self._client is not None
        return BackendInfo(
            name="mem0",
            available=ok,
            reason="" if ok else self._reason,
            includes_network=self._mode != "oss",
            offline=self._mode == "oss",
        )

    def reset(self) -> None:
        return None

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        assert self._client is not None
        r = self._client.add(content, user_id=self._user_id, metadata=metadata or {})
        if isinstance(r, dict):
            return str(r.get("id") or r.get("results", [{}])[0].get("id", "mem0"))
        return "mem0"

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        assert self._client is not None
        r = self._client.search(query, user_id=self._user_id, limit=top_k)
        results = r if isinstance(r, list) else r.get("results", [])
        hits: list[SearchHit] = []
        for i, item in enumerate(results[:top_k]):
            if isinstance(item, dict):
                hits.append(
                    SearchHit(
                        id=str(item.get("id", i)),
                        content=str(item.get("memory") or item.get("content") or ""),
                        score=float(item.get("score") or 0.0),
                    )
                )
        return hits
