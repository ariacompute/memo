from __future__ import annotations

import math
import re
from collections import Counter

_ARTICLES = {"a", "an", "the"}


def normalize_text(text: str) -> list[str]:
    """Normalize per LoCoMo-Refined convention: lowercase, strip punctuation and
    articles, split on whitespace."""
    text = text.lower()
    # keep alphanumerics and whitespace; replace everything else with a space
    text = re.sub(r"[^a-z0-9\s]", " ", text)
    tokens = [t for t in text.split() if t and t not in _ARTICLES]
    return tokens


def token_f1(prediction: str, ground_truth: str) -> float:
    """Token-level F1 (LoCoMo-Refined convention: single answer, overlap of prediction and ground truth)."""
    pred_tok = normalize_text(prediction)
    gt_tok = normalize_text(ground_truth)
    if not pred_tok and not gt_tok:
        return 1.0
    if not pred_tok or not gt_tok:
        return 0.0
    common = Counter(pred_tok) & Counter(gt_tok)
    overlap = sum(common.values())
    if overlap == 0:
        return 0.0
    precision = overlap / len(pred_tok)
    recall = overlap / len(gt_tok)
    return 2 * precision * recall / (precision + recall)


def _bleu_single(prediction: str, reference: str) -> float:
    """Single-reference BLEU (with shortest-reference smoothing)."""
    pred = normalize_text(prediction)
    ref = normalize_text(reference)
    if not pred:
        return 0.0
    if not ref:
        return 0.0
    # 1-gram precision
    pred_counts = Counter(pred)
    ref_counts = Counter(ref)
    clipped = {w: min(c, ref_counts.get(w, 0)) for w, c in pred_counts.items()}
    overlap = sum(clipped.values())
    if overlap == 0:
        return 0.0
    precision = overlap / len(pred)
    # brevity penalty (only when prediction is shorter than reference)
    bp = 1.0
    if len(pred) < len(ref):
        bp = math.exp(1 - len(ref) / len(pred))
    return bp * precision


def bleu(prediction: str, references: list[str]) -> float:
    """Take the max BLEU over multiple candidates."""
    if not references:
        return 0.0
    return max(_bleu_single(prediction, r) for r in references)


def multiple_candidate_max(fn, prediction: str, ground_truths: list[str]) -> float:
    """Take the max metric over multiple ground-truth candidates (LoCoMo-Refined picks the best for multi-answer)."""
    if not ground_truths:
        return 0.0
    return max(fn(prediction, g) for g in ground_truths)
