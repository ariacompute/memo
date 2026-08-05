from __future__ import annotations

from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


class LettaBackend(MemoBackend):
    def __init__(self) -> None:
        self._client: Any = None
        self._reason = ""
        try:
            from letta import create_client  # type: ignore

            self._client = create_client()
        except Exception as e:  # noqa: BLE001
            self._reason = f"letta unavailable ({e}); pip install letta and configure server"

    def info(self) -> BackendInfo:
        ok = self._client is not None
        return BackendInfo(
            name="letta",
            available=ok,
            reason="" if ok else self._reason,
            includes_network=True,
            offline=False,
        )

    def reset(self) -> None:
        return None

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        _ = content, metadata
        raise RuntimeError("Letta is an agent runtime; map archival memo API before enabling")

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        _ = query, top_k
        raise RuntimeError("Letta search not wired")
