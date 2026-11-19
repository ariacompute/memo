from __future__ import annotations

import json
import os
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters.agentmem import AgentMemBackend


class _Handler(BaseHTTPRequestHandler):
    calls: list[tuple[str, dict]] = []

    def log_message(self, *args):
        return

    def _reply(self, code: int, payload: dict) -> None:
        body = json.dumps(payload).encode("utf-8")
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path == "/healthz":
            self._reply(200, {"status": "ok"})
        else:
            self._reply(404, {"error": "not found"})

    def do_POST(self):
        length = int(self.headers.get("Content-Length", "0"))
        payload = json.loads(self.rfile.read(length)) if length else {}
        self.calls.append((self.path, payload))
        if self.path == "/v1/knowledge":
            self._reply(200, {"concept_id": payload.get("concept_id", "")})
        elif self.path == "/v1/migrate":
            self._reply(200, {"job": "bench", "target": "memory", "processed": 1})
        elif self.path == "/v1/query":
            self._reply(
                200,
                {
                    "hits": [
                        {
                            "concept_id": "doc1",
                            "score": 0.75,
                            "snippet": "hello world",
                            "source": "memory",
                            "citation": "c1",
                        }
                    ]
                },
            )
        else:
            self._reply(404, {"error": "not found"})


class TestAgentMemAdapter(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.server = ThreadingHTTPServer(("127.0.0.1", 0), _Handler)
        cls.thread = threading.Thread(target=cls.server.serve_forever, daemon=True)
        cls.thread.start()
        cls.base_url = f"http://127.0.0.1:{cls.server.server_port}"

    @classmethod
    def tearDownClass(cls):
        cls.server.shutdown()
        cls.server.server_close()
        cls.thread.join(timeout=5)

    def setUp(self):
        _Handler.calls = []

    def test_missing_binary_is_unavailable_with_reason(self):
        with mock.patch.dict(os.environ, {"AGENTMEM_BIN": ""}, clear=False), mock.patch(
            "shutil.which", return_value=None
        ):
            info = AgentMemBackend().info()
        self.assertFalse(info.available)
        self.assertIn("AGENTMEM_BIN", info.reason)
        self.assertFalse(info.includes_network)
        self.assertTrue(info.offline)

    def test_add_and_search_over_local_http(self):
        backend = AgentMemBackend(bin_path="unused")
        backend._base_url = self.base_url
        try:
            self.assertEqual(backend.add("hello", {"key": "doc1"}), "doc1")
            hits = backend.search("hello", 5)
        finally:
            backend.close()
        self.assertEqual(len(hits), 1)
        self.assertEqual(hits[0].id, "doc1")
        self.assertEqual(hits[0].content, "hello world")
        self.assertAlmostEqual(hits[0].score, 0.75)

        paths = [path for path, _ in _Handler.calls]
        self.assertIn("/v1/knowledge", paths)
        self.assertIn("/v1/query", paths)
        put_payload = next(p for path, p in _Handler.calls if path == "/v1/knowledge")
        self.assertEqual(put_payload["document"]["body"], "hello")
        query_payload = next(p for path, p in _Handler.calls if path == "/v1/query")
        self.assertEqual(query_payload["top_k"], 5)

    def test_startup_failure_reports_exit_code_and_cleans_up(self):
        backend = AgentMemBackend(bin_path="/bin/false", timeout_s=2)
        with self.assertRaises(RuntimeError) as ctx:
            backend.reset()
        self.assertIn("agentmem serve exited with", str(ctx.exception))
        self.assertIsNone(backend._base_url)
        self.assertIsNone(backend._tmpdir)

    def test_http_error_propagates(self):
        backend = AgentMemBackend(bin_path="unused")
        backend._base_url = self.base_url + "/missing"
        with self.assertRaises(RuntimeError):
            backend.search("hello", 5)
        backend.close()


if __name__ == "__main__":
    unittest.main()
