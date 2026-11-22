from __future__ import annotations

import contextlib
import io
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


class RecordingBackend(FakeBackend):
    """FakeBackend that records update targets and can fail on demand."""

    def __init__(self, supports_update=True, fail_update=False, fail_list=False):
        super().__init__(supports_update=supports_update)
        self.updated = []
        self._fail_update = fail_update
        self._fail_list = fail_list

    def list_memories(self) -> list[SearchHit]:
        if self._fail_list:
            raise RuntimeError("list_memories unavailable")
        return super().list_memories()

    def update(self, memo_id: str, content=None, importance=None) -> None:
        self.updated.append((memo_id, content))
        if self._fail_update:
            raise RuntimeError("backend rejected update")
        super().update(memo_id, content, importance)


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

    # ---------- memory updating: upstream MemoryID -> backend id addressing ----------

    def _update_score(self, scores, name="update_accuracy"):
        hits = [s for s in scores if s.name == name and s.subset == "updating"]
        self.assertTrue(hits, f"expected an updating-subset {name} score")
        return hits[0]

    def _fixture_dataset(self, mod):
        return mod.load(resolve_dataset("halumem").path, limit=2)

    def test_halumem_update_uses_backend_id_mapping(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        ds = self._fixture_dataset(mod)
        backend = RecordingBackend()
        scores = mod.run(backend, ds, None, 5)
        score = self._update_score(scores)
        # The update must target the id the backend generated at ingest time, not the
        # upstream MemoryID (which the backend has never seen).
        self.assertTrue(backend.updated, "update was never attempted")
        target = backend.updated[0][0]
        self.assertIn(target, backend.ids)
        self.assertEqual(target, backend.ids[0])
        self.assertNotEqual(target, str(ds.memories[0].get("MemoryID")))
        self.assertEqual(score.value, 1.0)
        self.assertIn("id_unmapped=0", score.reason)
        self.assertIn("update_failed=0", score.reason)

    def test_halumem_update_falls_back_to_list_memories_without_ingest(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        backend = RecordingBackend()
        backend.reset()
        seeded = backend.add("The user lives in Berlin.")
        ds = mod.Dataset(
            memories=[{"MemoryID": 1, "Content": "The user lives in Berlin."}],
            sessions=[],
            questions=[
                {
                    "Question": "Where does the user live?",
                    "Answer": ["Munich"],
                    "UpdateMemoryID": 1,
                    "UpdateType": "update",
                }
            ],
        )
        scores = mod.run(backend, ds, None, 5, do_ingest=False)
        score = self._update_score(scores)
        self.assertEqual(backend.updated[0][0], seeded)
        self.assertEqual(score.value, 1.0)
        self.assertIn("id_unmapped=0", score.reason)

    def test_halumem_update_reports_unmapped_id(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        backend = RecordingBackend()
        ds = mod.Dataset(
            memories=[{"MemoryID": 7, "Content": "The user lives in Berlin."}],
            sessions=[],
            questions=[
                {
                    "Question": "Where does the user live?",
                    "Answer": ["Munich"],
                    "UpdateMemoryID": 99,  # no such upstream memory
                    "UpdateType": "update",
                }
            ],
        )
        scores = mod.run(backend, ds, None, 5)
        score = self._update_score(scores)
        self.assertEqual(score.value, 0.0)
        self.assertIn("id_unmapped=1", score.reason)
        self.assertIn("UpdateMemoryID=99", score.reason)
        self.assertEqual(backend.updated, [])

    def test_halumem_update_failure_is_reported_not_swallowed(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        ds = self._fixture_dataset(mod)
        backend = RecordingBackend(fail_update=True)
        scores = mod.run(backend, ds, None, 5)
        score = self._update_score(scores)
        hallucination = self._update_score(scores, "hallucination_rate")
        self.assertEqual(score.value, 0.0)
        self.assertEqual(hallucination.value, 1.0)
        self.assertIn("update_failed=1", score.reason)
        self.assertIn("backend rejected update", score.reason)

    def test_halumem_update_degrades_when_list_memories_fails(self):
        mod = __import__("track_b.halumem", fromlist=["load", "run"])
        backend = RecordingBackend(fail_list=True)
        ds = mod.Dataset(
            memories=[{"MemoryID": 1, "Content": "The user lives in Berlin."}],
            sessions=[],
            questions=[
                {
                    "Question": "Where does the user live?",
                    "Answer": ["Munich"],
                    "UpdateMemoryID": 1,
                    "UpdateType": "update",
                }
            ],
        )
        err = io.StringIO()
        with contextlib.redirect_stderr(err):
            scores = mod.run(backend, ds, None, 5, do_ingest=False)
        score = self._update_score(scores)
        # The fallback cannot resolve the id, but it must degrade loudly, not crash.
        self.assertEqual(score.value, 0.0)
        self.assertIn("id_unmapped=1", score.reason)
        self.assertEqual(backend.updated, [])
        self.assertIn("list_memories failed", err.getvalue())

    def test_dry_run_loads_all_benchmarks(self):
        backend = FakeBackend()
        report = dry_run(backend, out_dir=None, benchmarks=list(BENCHMARKS))
        self.assertEqual(report["mode"], "dry-run")
        bench_names = {b["benchmark"] for b in report["benchmarks"]}
        self.assertEqual(bench_names, set(BENCHMARKS))


if __name__ == "__main__":
    unittest.main()
