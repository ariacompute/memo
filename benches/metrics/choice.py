from __future__ import annotations

import re

# 匹配每个选项：字母 + ")" 或 "." + 其后文本（到下一个选项或结尾）
_TOKEN = re.compile(r"([A-Za-z])\s*[\.\)]\s*(.*?)(?=\s*[A-Za-z]\s*[\.\)]|$)", re.DOTALL)


def _split_options(text: str) -> dict[str, str]:
    """将 'A) foo B) bar' 或逐行 'A) foo\nB) bar' 解析为 {letter: option_text}。"""
    out: dict[str, str] = {}
    for m in _TOKEN.finditer(text):
        out[m.group(1).upper()] = m.group(2).strip()
    # 退回到逐行解析
    if not out:
        for line in text.splitlines():
            lm = re.match(r"^\s*([A-Za-z])\s*[\.\)]?\s*(.*)$", line, re.DOTALL)
            if lm:
                out[lm.group(1).upper()] = lm.group(2).strip()
    return out


def choice_hit(question_options: str | dict[str, str], gold_letter: str, prediction: str) -> str | None:
    """返回命中的选项字母，否则 None。

    - question_options: 题目选项文本（含 A) B) ...）或已解析的 {letter: text}。
    - gold_letter: 正确选项字母。
    - prediction: 系统输出（可能是字母，或选项文本片段，或检索回的内容）。
    """
    opts = question_options if isinstance(question_options, dict) else _split_options(question_options)
    gold = gold_letter.strip().upper()
    pred = prediction.strip()

    # 1) 直接字母命中
    m = re.match(r"^\s*([A-Za-z])\b", pred)
    if m and m.group(1).upper() == gold:
        return gold

    # 2) 选项文本包含匹配（预测包含正确选项文本）
    gold_text = opts.get(gold, "")
    if gold_text and gold_text.lower() in pred.lower():
        return gold

    # 3) 任一选项文本与预测高度重合则判该选项；否则 None
    for letter, text in opts.items():
        if text and text.lower() in pred.lower():
            return letter
    return None


def multiple_choice_accuracy(
    rows: list[tuple[str | dict[str, str], str, str]]
) -> tuple[float, int, int]:
    """批量计算多选准确率。

    每行：(question_options, gold_letter, prediction)。
    返回 (accuracy, correct, total)。
    """
    total = len(rows)
    if total == 0:
        return 0.0, 0, 0
    correct = 0
    for opts, gold, pred in rows:
        if choice_hit(opts, gold, pred) == gold:
            correct += 1
    return correct / total, correct, total
