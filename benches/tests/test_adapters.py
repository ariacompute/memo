from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters import build_backend
from adapters.aria_memo import AriaMemoBackend
from adapters.base import BackendInfo, MemoBackend, UnsupportedCapability
from adapters.skip import SkipBackend


class TestAdapters(unittest.TestCase):
    def test_skip_backend_unavailable(self):
        b = SkipBackend("mem0", "no creds")
        self.assertFalse(b.info().available)
        with self.assertRaises(RuntimeError):
            b.add("x")

    def test_default_optional_caps_unsupported(self):
        class Minimal(MemoBackend):
            def info(self):
                return BackendInfo(name="x", available=True)

            def reset(self):
                pass

            def add(self, content, metadata=None):
                return "id"

            def search(self, query, top_k=5):
                return []

        m = Minimal()
        self.assertFalse(m.supports("list_memories"))
        self.assertFalse(m.supports("update"))
        with self.assertRaises(UnsupportedCapability):
            m.list_memories()
        with self.assertRaises(UnsupportedCapability):
            m.update("id", content="x")


class TestControlAdapters(unittest.TestCase):
    def test_sqlite_vec_unavailable_without_dep(self):
        b = build_backend("sqlite_vec")
        info = b.info()
        self.assertEqual(info.name, "sqlite_vec")
        # In this environment sqlite-vec is not installed -> skip with reason, offline.
        if not info.available:
            self.assertFalse(info.includes_network)
            self.assertTrue(info.offline)
            self.assertTrue(info.reason)

    def test_chromem_unavailable_without_binary(self):
        b = build_backend("chromem")
        info = b.info()
        self.assertEqual(info.name, "chromem")
        # No chromem-go binary on PATH here -> skip with reason.
        if not info.available:
            self.assertFalse(info.includes_network)
            self.assertTrue(info.reason)

    def test_control_adapters_registered(self):
        self.assertIsNotNone(build_backend("sqlite_vec"))
        self.assertIsNotNone(build_backend("chromem"))

    def test_agentmem_not_registered(self):
        for name in ("agentmem", "agent-mem"):
            backend = build_backend(name)
            self.assertIsInstance(backend, SkipBackend)
            self.assertFalse(backend.info().available)
            self.assertIn("unknown system", backend.info().reason)

    def test_removed_systems_skip_with_reason(self):
        for name in ("memos", "mempalace"):
            backend = build_backend(name)
            self.assertIsInstance(backend, SkipBackend)
            self.assertFalse(backend.info().available)
            self.assertIn("unknown system", backend.info().reason)


class TestAriaMemoAdapter(unittest.TestCase):
    def test_benchmark_type_labels_are_normalized(self):
        backend = AriaMemoBackend(bin_path="unused")
        with mock.patch.object(backend, "_run", return_value="doc1") as run:
            self.assertEqual(backend.add("x", {"memo_type": "event"}), "doc1")
        self.assertEqual(run.call_args.args[:3], ("add", "--type", "working"))

        with mock.patch.object(backend, "_run", return_value="doc2") as run:
            backend.add("y", {"memo_type": "long_term:semantic"})
        self.assertEqual(run.call_args.args[:3], ("add", "--type", "long_term:semantic"))


if __name__ == "__main__":
    unittest.main()
