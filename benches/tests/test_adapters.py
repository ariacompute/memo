from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters import build_backend
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


if __name__ == "__main__":
    unittest.main()
