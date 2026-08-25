from __future__ import annotations

"""离线指标：词法 F1/BLEU、多选准确率、检索 Recall@k / MRR。

所有函数纯标准库实现，无外部依赖；可在无 LLM、无数据集 fixture 情况下单测。
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
