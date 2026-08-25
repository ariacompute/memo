from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from adapters.base import BackendInfo, MemoBackend, SearchHit, UnsupportedCapability
from judge import Judge
from track_b import BENCHMARKS, dry_run, run_track_b
from datasets import resolve_dataset


class FakeBackend(MemoBackend):
    def __init__(self, supports_update=False):
        self.store = []
        self.ids = []
        self._supports = {"list_memories": True, "update": supports_update}

    def info(self) -> BackendInfo:
        return BackendInfo(name="fake", available=True, offline=True)

    def reset(self):
        self.store = []
        self.ids = []

    def add(self, content: str, metadata=None) -> str:
        self.store.append(content)
        mid = f"m{len(self.ids)}"
        self.ids.append(mid)
        return mid

    def search(self, query: str, top_k: int = 5) -> list[SearchHit]:
        hits = [s for s in self.store if query.lower() in s.lower()]
        return [SearchHit(id=f"m{i}", content=s, score=1.0) for i, s in enumerate(hits)][:top_k]

    def list_memories(self) -> list[SearchHit]:
        return [SearchHit(id=i, content=c, score=1.0) for i, c in zip(self.ids, self.store)]

    def supports(self, cap: str) -> bool:
        return self._supports.get(cap, False)

    def update(self, memo_id: str, content=None, importance=None) -> None:
        if not self._supports.get("update"):
            raise UnsupportedCapability("update")
        if content is not None and self.store:
            self.store[0] = content


class FakeJudge(Judge):
    def __init__(self, verdict=True):
        super().__init__(api_key="fake", model="fake-model", base_url="http://x")
        self._verdict = verdict

    def judge(self, question, gold, pred) -> bool:
        self.stats.calls += 1
        ok = self._verdict
        if ok:
            self.stats.yes += 1
        return ok


def _run_one(bench, judge):
    mod = __import__(f"track_b.{bench}", fromlist=["load", "run"])
    resolved = resolve_dataset(bench)
    ds = mod.load(resolved.path, limit=2)
    backend = FakeBackend(supports_update=True)
    return mod.run(backend, ds, judge, 5)


class TestTrackB(unittest.TestCase):
    def test_registry_has_four_benchmarks(self):
        self.assertEqual(
            set(BENCHMARKS), {"locomo_refined", "halumem", "longmemeval", "personamem"}
        )

    def test_run_track_b_with_fake_judge_returns_scores(self):
        backend = FakeBackend(supports_update=True)
        judge = FakeJudge(verdict=True)
        summary = run_track_b(
            backend, judge, out_dir=None, benchmarks=["personamem"], top_k=5, limit=2
        )
        self.assertEqual(summary["track"], "B")
        self.assertTrue(summary["judge"]["available"])
        scores = summary["systems"][0]["scores"]
        names = {s["name"] for s in scores}
        self.assertIn("multiple_choice_accuracy", names)

    def test_judge_none_skips_judge_metrics(self):
        scores = _run_one("locomo_refined", None)
        ja = [s for s in scores if s.name == "judge_accuracy"]
        self.assertTrue(ja and ja[0].skipped and ja[0].value is None)

    def test_halumem_update_capability_downgrade(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        resolved = resolve_dataset("halumem")
        ds = mod.load(resolved.path, limit=2)
        backend = FakeBackend(supports_update=False)  # update unsupported
        scores = mod.run(backend, ds, None, 5)
        upd = [s for s in scores if s.name == "update_accuracy"]
        self.assertTrue(upd and upd[0].skipped)

    def test_dry_run_loads_all_benchmarks(self):
        backend = FakeBackend()
        report = dry_run(backend, out_dir=None, benchmarks=list(BENCHMARKS))
        self.assertEqual(report["mode"], "dry-run")
        bench_names = {b["benchmark"] for b in report["benchmarks"]}
        self.assertEqual(bench_names, set(BENCHMARKS))


if __name__ == "__main__":
    unittest.main()
