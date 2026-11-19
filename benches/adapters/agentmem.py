from __future__ import annotations

import json
import os
import shutil
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

from .base import BackendInfo, MemoBackend, SearchHit


def _find_bin() -> str | None:
    env = os.environ.get("AGENTMEM_BIN")
    if env and Path(env).is_file():
        return env
    return shutil.which("agentmem")


def _free_port() -> int:
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(("127.0.0.1", 0))
        return int(s.getsockname()[1])


class AgentMemBackend(MemoBackend):
    """Drive a local agentmem HTTP service started with an isolated offline config."""

    def __init__(self, bin_path: str | None = None, timeout_s: float | None = None) -> None:
        self._bin = bin_path or _find_bin()
        self._timeout_s = timeout_s if timeout_s is not None else 30.0
        self._tmpdir: tempfile.TemporaryDirectory[str] | None = None
        self._proc: subprocess.Popen[bytes] | None = None
        self._base_url: str | None = None
        self._n = 0
        self._dirty = False

    def info(self) -> BackendInfo:
        if not self._bin:
            return BackendInfo(
                name="agentmem",
                available=False,
                reason=(
                    "agentmem binary not found; build with "
                    "`go build -o bin/agentmem ./cmd/agentmem` and set AGENTMEM_BIN"
                ),
                includes_network=False,
                offline=True,
            )
        return BackendInfo(name="agentmem", available=True, includes_network=False, offline=True)

    def reset(self) -> None:
        self.close()
        info = self.info()
        if not info.available or not self._bin:
            raise RuntimeError(info.reason)
        self._tmpdir = tempfile.TemporaryDirectory(prefix="agentmem-bench-")
        root = Path(self._tmpdir.name)
        port = _free_port()
        env_path = root / "agentmem.env"
        env_path.write_text(
            "\n".join(
                [
                    f"AGENTMEM_ADDR=127.0.0.1:{port}",
                    f"AGENTMEM_GIT_ROOT={root / 'bundle'}",
                    "AGENTMEM_MODE=dev",
                    "AGENTMEM_AUTH_ENABLED=false",
                    "AGENTMEM_PG_DSN=",
                    "AGENTMEM_VECTOR_DSN=",
                    "AGENTMEM_SYSTEMONE_ENDPOINT=",
                ]
            )
            + "\n",
            encoding="utf-8",
        )
        self._base_url = f"http://127.0.0.1:{port}"
        self._dirty = False
        err_path = root / "agentmem.stderr"
        # agentmem rejects unknown AGENTMEM_* process keys; the isolated env file is
        # the only source of configuration for the child.
        child_env = {k: v for k, v in os.environ.items() if not k.startswith("AGENTMEM_")}
        with err_path.open("wb") as err:
            self._proc = subprocess.Popen(
                [self._bin, "serve", "--env", str(env_path)],
                stdout=subprocess.DEVNULL,
                stderr=err,
                env=child_env,
            )
        self._wait_ready(err_path)

    def _wait_ready(self, err_path: Path) -> None:
        deadline = time.monotonic() + self._timeout_s
        last_error = "agentmem serve did not become ready"
        while time.monotonic() < deadline:
            if self._proc is not None and self._proc.poll() is not None:
                detail = err_path.read_text(encoding="utf-8", errors="replace").strip()
                code = self._proc.returncode
                self.close()
                raise RuntimeError(detail or f"agentmem serve exited with {code}")
            try:
                self._request("GET", "/healthz")
                return
            except (urllib.error.URLError, RuntimeError, TimeoutError, ValueError) as exc:
                last_error = str(exc)
                time.sleep(0.1)
        self.close()
        raise RuntimeError(f"{last_error} within {self._timeout_s}s")

    def _request(self, method: str, path: str, payload: dict[str, Any] | None = None) -> Any:
        if not self._base_url:
            raise RuntimeError("agentmem service not started")
        data = json.dumps(payload).encode("utf-8") if payload is not None else None
        req = urllib.request.Request(
            f"{self._base_url}{path}",
            data=data,
            method=method,
            headers={"Content-Type": "application/json"},
        )
        try:
            with urllib.request.urlopen(req, timeout=self._timeout_s) as response:
                body = response.read()
        except urllib.error.HTTPError as exc:
            raise RuntimeError(f"agentmem HTTP {exc.code}") from None
        except urllib.error.URLError as exc:
            raise RuntimeError(f"agentmem request failed: {exc.reason}") from None
        return json.loads(body) if body else {}

    def _ensure(self) -> None:
        if self._base_url is None:
            self.reset()

    def _sync(self) -> None:
        """Project pending canonical writes into the local memory adapter before querying."""
        if not self._dirty:
            return
        self._request(
            "POST",
            "/v1/migrate",
            {
                "job": "bench",
                "direction": "canonical_to_rag",
                "target": "memory",
                "dry_run": False,
                "limit": 1_000_000,
            },
        )
        self._dirty = False

    def add(self, content: str, metadata: dict[str, Any] | None = None) -> str:
        self._ensure()
        meta = metadata or {}
        self._n += 1
        concept_id = str(meta.get("key") or meta.get("id") or f"bench-{self._n}")
        payload = {
            "concept_id": concept_id,
            "document": {
                "type": "Concept",
                "title": concept_id,
                "description": "",
                "body": content,
                "tags": [],
            },
        }
        result = self._request("POST", "/v1/knowledge", payload)
        self._dirty = True
        return str(result.get("concept_id", concept_id))

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        self._ensure()
        self._sync()
        bounded_k = max(1, min(int(top_k), 100))
        result = self._request("POST", "/v1/query", {"query": query, "top_k": bounded_k})
        hits: list[SearchHit] = []
        for item in result.get("hits") or []:
            hits.append(
                SearchHit(
                    id=str(item.get("concept_id", "")),
                    content=str(item.get("snippet") or item.get("concept_id") or ""),
                    score=float(item.get("score", 0.0)),
                    metadata={
                        "source": item.get("source", ""),
                        "citation": item.get("citation", ""),
                    },
                )
            )
        return hits

    def close(self) -> None:
        if self._proc is not None:
            if self._proc.poll() is None:
                self._proc.terminate()
                try:
                    self._proc.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self._proc.kill()
                    self._proc.wait(timeout=5)
            self._proc = None
        if self._tmpdir is not None:
            self._tmpdir.cleanup()
            self._tmpdir = None
        self._base_url = None
