from __future__ import annotations

import re

# match each option: letter + ")" or "." + its text (up to the next option or end)
_TOKEN = re.compile(r"([A-Za-z])\s*[\.\)]\s*(.*?)(?=\s*[A-Za-z]\s*[\.\)]|$)", re.DOTALL)


def _split_options(text: str) -> dict[str, str]:
    """Parse 'A) foo B) bar' or line-by-line 'A) foo\nB) bar' into {letter: option_text}."""
    out: dict[str, str] = {}
    for m in _TOKEN.finditer(text):
        out[m.group(1).upper()] = m.group(2).strip()
    # fall back to line-by-line parsing
    if not out:
        for line in text.splitlines():
            lm = re.match(r"^\s*([A-Za-z])\s*[\.\)]?\s*(.*)$", line, re.DOTALL)
            if lm:
                out[lm.group(1).upper()] = lm.group(2).strip()
    return out


def choice_hit(question_options: str | dict[str, str], gold_letter: str, prediction: str) -> str | None:
    """Return the hit option letter, or None.

    - question_options: the question's option text (with A) B) ...) or an already
      parsed {letter: text}.
    - gold_letter: the correct option letter.
    - prediction: the system output (may be a letter, an option text fragment, or
      retrieved content).
    """
    opts = question_options if isinstance(question_options, dict) else _split_options(question_options)
    gold = gold_letter.strip().upper()
    pred = prediction.strip()

    # 1) direct letter hit
    m = re.match(r"^\s*([A-Za-z])\b", pred)
    if m and m.group(1).upper() == gold:
        return gold

    # 2) option-text containment (prediction contains the correct option text)
    gold_text = opts.get(gold, "")
    if gold_text and gold_text.lower() in pred.lower():
        return gold

    # 3) if any option text strongly overlaps the prediction, judge that option; else None
    for letter, text in opts.items():
        if text and text.lower() in pred.lower():
            return letter
    return None


def multiple_choice_accuracy(
    rows: list[tuple[str | dict[str, str], str, str]]
) -> tuple[float, int, int]:
    """Compute multi-choice accuracy over a batch.

    Each row: (question_options, gold_letter, prediction).
    Returns (accuracy, correct, total).
    """
    total = len(rows)
    if total == 0:
        return 0.0, 0, 0
    correct = 0
    for opts, gold, pred in rows:
        if choice_hit(opts, gold, pred) == gold:
            correct += 1
    return correct / total, correct, total
