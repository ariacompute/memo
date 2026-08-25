from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from metrics import (
    bleu,
    choice_hit,
    mrr,
    multiple_candidate_max,
    multiple_choice_accuracy,
    normalize_text,
    recall_at_k,
    retrieval_hit_rate,
    token_f1,
)


class TestMetrics(unittest.TestCase):
    def test_normalize_lowercases_and_strips(self):
        self.assertEqual(normalize_text("The User's Cat!"), ["user", "s", "cat"])

    def test_token_f1_exact(self):
        self.assertEqual(token_f1("rust programming", "rust programming"), 1.0)

    def test_token_f1_partial(self):
        f = token_f1("rust python", "rust")
        self.assertTrue(0.0 < f < 1.0)

    def test_token_f1_empty(self):
        self.assertEqual(token_f1("", ""), 1.0)
        self.assertEqual(token_f1("x", ""), 0.0)
        self.assertEqual(token_f1("", "x"), 0.0)

    def test_bleu_multicandidate(self):
        self.assertEqual(bleu("rust", ["python", "rust"]), 1.0)
        self.assertLessEqual(bleu("rust", ["java"]), 1.0)

    def test_multiple_candidate_max(self):
        self.assertEqual(multiple_candidate_max(token_f1, "rust", ["python", "rust"]), 1.0)

    def test_choice_hit_letter(self):
        self.assertEqual(choice_hit("A) veg B) vegan", "A", "A"), "A")

    def test_choice_hit_text_match(self):
        self.assertEqual(choice_hit("A) vegetarian B) vegan", "A", "the user is vegetarian"), "A")

    def test_choice_hit_wrong(self):
        self.assertNotEqual(choice_hit("A) vegetarian B) vegan", "A", "they are vegan"), "A")

    def test_multiple_choice_accuracy(self):
        rows = [
            ("A) veg B) vegan", "A", "A"),
            ("A) veg B) vegan", "B", "B"),
            ("A) veg B) vegan", "A", "B"),
        ]
        acc, correct, total = multiple_choice_accuracy(rows)
        self.assertEqual(total, 3)
        self.assertEqual(correct, 2)
        self.assertAlmostEqual(acc, 2 / 3)

    def test_recall_at_k(self):
        # 任一相关项命中即算召回（标准 per-query Recall@k）
        self.assertEqual(recall_at_k(["a", "b"], ["x", "a", "y"], 2), 1.0)
        self.assertEqual(recall_at_k(["a", "b"], ["x", "y", "z"], 3), 0.0)
        self.assertEqual(recall_at_k([], ["a"], 3), 0.0)

    def test_mrr(self):
        self.assertEqual(mrr(["a"], ["x", "a"], 3), 0.5)
        self.assertEqual(mrr(["a"], ["a", "x"], 3), 1.0)
        self.assertEqual(mrr(["a"], ["x", "y"], 3), 0.0)

    def test_retrieval_hit_rate(self):
        pairs = [(["a"], ["x", "a"]), (["b"], ["y", "z"])]
        r, hits, total = retrieval_hit_rate(pairs, 3)
        self.assertEqual(total, 2)
        self.assertEqual(hits, 1)
        self.assertAlmostEqual(r, 0.5)


if __name__ == "__main__":
    unittest.main()
