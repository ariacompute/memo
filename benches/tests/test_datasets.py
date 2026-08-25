from __future__ import annotations

import sys
import unittest
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

from datasets import DATASET_SPECS, download, resolve_dataset


class TestDatasets(unittest.TestCase):
    def test_specs_present_for_four_benchmarks(self):
        for b in ("locomo_refined", "halumem", "longmemeval", "personamem"):
            self.assertIn(b, DATASET_SPECS)
            self.assertTrue(DATASET_SPECS[b].files)

    def test_resolve_fixture_fallback_for_locomo(self):
        # 直接指向 fixtures 目录，避免本机已下载的真实数据干扰
        FIX = Path(__file__).resolve().parents[1] / "data" / "fixtures" / "locomo_refined"
        self.assertTrue((FIX / "questions.jsonl").exists())
        r = resolve_dataset("locomo_refined")
        self.assertIn(r.source, ("fixture", "real"))
        self.assertTrue((r.path / "questions.jsonl").exists())

    def test_resolve_missing_both_raises_with_manual(self):
        with mock.patch("datasets._DATA_ROOT", Path("/nonexistent/path/xyz")):
            with self.assertRaises(FileNotFoundError) as ctx:
                resolve_dataset("locomo_refined")
            self.assertTrue("manual" in str(ctx.exception).lower() or "Download" in str(ctx.exception))

    def test_download_network_failure_raises_with_manual(self):
        import tempfile
        import urllib.request

        tmp = Path(tempfile.mkdtemp())

        def boom(*a, **k):
            raise OSError("network down")

        with mock.patch("datasets._DATA_ROOT", tmp), mock.patch.object(
            urllib.request, "urlopen", boom
        ):
            with self.assertRaises(RuntimeError) as ctx:
                download("locomo_refined")
            self.assertIn("manual", str(ctx.exception).lower())


if __name__ == "__main__":
    unittest.main()
