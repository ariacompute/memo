from __future__ import annotations

"""Offline metrics: lexical F1/BLEU, multiple-choice accuracy, retrieval Recall@k / MRR.

All functions are implemented with the standard library only (no external
dependencies) and can be unit-tested without an LLM or dataset fixtures.
"""

from .lexical import (
    normalize_text,
    token_f1,
    bleu,
    multiple_candidate_max,
)
from .choice import multiple_choice_accuracy, choice_hit
from .retrieval import recall_at_k, mrr, retrieval_hit_rate

__all__ = [
    "normalize_text",
    "token_f1",
    "bleu",
    "multiple_candidate_max",
    "multiple_choice_accuracy",
    "choice_hit",
    "recall_at_k",
    "mrr",
    "retrieval_hit_rate",
]
