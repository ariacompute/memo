from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from track_b import locomo_refined, halumem, longmemeval, personamem  # noqa: F401
from datasets import resolve_dataset

FIX = Path(__file__).resolve().parents[1] / "data" / "fixtures"


class TestLoaders(unittest.TestCase):
    def test_locomo_loader_fixture(self):
        ds = locomo_refined.load(FIX / "locomo_refined")
        self.assertEqual(ds.size, 4)
        self.assertIn("s1", ds.conversations)
        self.assertTrue(ds.questions[0]["question"])

    def test_halumem_loader_fixture(self):
        ds = halumem.load(FIX / "halumem")
        self.assertGreaterEqual(ds.size, 1)
        self.assertTrue(ds.memories)
        self.assertTrue(ds.sessions)

    def test_halumem_loader_real_format(self):
        import json
        import tempfile

        rec = {
            "uuid": "u1",
            "sessions": [
                {
                    "memory_points": [
                        {"memory_content": "Name is Martin", "memory_type": "Persona Memory",
                         "is_update": "False", "importance": 0.8},
                        {"memory_content": "Old job", "memory_type": "Event Memory",
                         "is_update": "True", "importance": 0.5},
                    ],
                    "dialogue": [{"role": "user", "content": "I am Martin."}],
                    "questions": [
                        {"question": "What is the name?", "answer": "Martin",
                         "question_type": "Persona"},
                    ],
                }
            ],
        }
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / "HaluMem-Medium.jsonl"
            p.write_text(json.dumps(rec) + "\n", encoding="utf-8")
            ds = halumem.load(p)
        self.assertEqual(ds.size, 1)
        self.assertEqual(len(ds.memories), 2)
        # is_update=True maps to Distraction=True (non-relevant memory)
        distract = [m for m in ds.memories if m.get("Distraction")]
        self.assertEqual(len(distract), 1)
        self.assertEqual(ds.sessions[0]["SessionID"], "u1")
        self.assertEqual(ds.questions[0]["Answer"], ["Martin"])

    def test_longmemeval_loader_fixture(self):
        ds = longmemeval.load(FIX / "longmemeval")
        self.assertEqual(ds.size, 2)
        self.assertIn("haystack_sessions", ds.items[0])

    def test_personamem_loader_fixture_and_truncation(self):
        ds = personamem.load(FIX / "personamem")
        self.assertEqual(ds.size, 1)
        q = ds.questions[0]
        ctx = ds.shared_contexts.get(q.shared_context_id, [])
        self.assertEqual(q.end_index, 1)
        self.assertGreaterEqual(len(ctx), 1)
        self.assertNotIn("Lisbon", ctx[: q.end_index])

    def test_resolve_fixture_fallback(self):
        r = resolve_dataset("locomo_refined")
        self.assertIn(r.source, ("fixture", "real"))
        self.assertTrue((r.path / "questions.jsonl").exists())


if __name__ == "__main__":
    unittest.main()
