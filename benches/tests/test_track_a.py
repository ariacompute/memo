from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters.base import BackendInfo, MemoBackend, SearchHit
from track_a import datasets as a2_datasets
from track_a import run_microbench, run_retrieval_quality

# run_retrieval_quality / run_microbench call `build_backend` via track_a's namespace,
# so patch there (not `adapters.build_backend`).
import track_a as track_a_mod


class FakeBackend(MemoBackend):
    """In-memory backend with trivial lexical search, for offline harness unit tests."""

    def __init__(self) -> None:
        self._docs: dict[str, str] = {}
        self._n = 0

    def info(self) -> BackendInfo:
        return BackendInfo(name="fake", available=True, includes_network=False, offline=True)

    def reset(self) -> None:
        self._docs = {}
        self._n = 0

    def add(self, content: str, metadata=None) -> str:  # type: ignore[override]
        self._n += 1
        cid = f"doc{self._n}"
        self._docs[cid] = content
        return cid

    def search(self, query: str, top_k: int = 5):
        q = query.lower()
        hits = []
        for cid, content in self._docs.items():
            if any(w in content.lower() for w in q.split() if w):
                hits.append(SearchHit(id=cid, content=content, score=1.0))
        return hits[:top_k]


def _patch(build):
    return mock.patch.object(track_a_mod, "build_backend", lambda name: build)


class TestA2Variance(unittest.TestCase):
    def test_load_dataset_unknown_is_none(self):
        self.assertIsNone(a2_datasets.load_dataset("does-not-exist"))

    def test_synthetic_v2_has_enough_queries(self):
        ds = a2_datasets.load_dataset("synthetic_v2")
        self.assertIsNotNone(ds)
        assert ds is not None
        self.assertGreaterEqual(len(ds["queries"]), 50)

    def test_track_b_locomo_loads(self):
        ds = a2_datasets.load_dataset("track_b:locomo_refined")
        self.assertIsNotNone(ds)
        assert ds is not None
        self.assertGreater(len(ds["queries"]), 0)

    def test_retrieval_quality_reports_variance(self):
        b = FakeBackend()
        with _patch(b):
            rep = run_retrieval_quality(["fake"], top_k=5, dataset="synthetic_v2")
        self.assertIn("systems", rep)
        row = next(r for r in rep["systems"] if r["name"] == "fake")
        self.assertTrue(row["available"])
        self.assertEqual(row["n_queries"], len(a2_datasets.load_dataset("synthetic_v2")["queries"]))
        # recall/mrr within [0,1]; variance fields present
        self.assertGreaterEqual(row["recall_at_k"], 0.0)
        self.assertLessEqual(row["recall_at_k"], 1.0)
        self.assertIn("recall_at_k_std", row)
        self.assertIn("mrr_std", row)

    def test_missing_dataset_skipped(self):
        with _patch(lambda _: FakeBackend()):
            rep = run_retrieval_quality(["fake"], top_k=5, dataset="nope")
        self.assertTrue(rep.get("skipped"))
        self.assertIn("reason", rep)


class TestA1Scaling(unittest.TestCase):
    def test_microbench_multi_size_and_scaling(self):
        b = FakeBackend()
        with _patch(b):
            rep = run_microbench(["fake"], sizes=[10, 20], top_k=5, warmup=0)
        self.assertEqual(rep["sizes"], [10, 20])
        # one row per (system, size)
        fake_rows = [r for r in rep["systems"] if r["name"] == "fake" and not r.get("skipped")]
        self.assertEqual(len(fake_rows), 2)
        self.assertIn("add", fake_rows[0])
        self.assertIn("search", fake_rows[0])
        # scaling entry present for the system
        self.assertIn("fake", rep["scaling"])
        self.assertIn("factors", rep["scaling"]["fake"])


if __name__ == "__main__":
    unittest.main()
