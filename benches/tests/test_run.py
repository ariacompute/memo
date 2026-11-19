from __future__ import annotations

import unittest

from run import DEFAULT_SIZES, DEFAULT_SYSTEMS, build_parser


class TestRunDefaults(unittest.TestCase):
    def test_defaults_align_with_spec(self):
        args = build_parser().parse_args([])
        self.assertEqual(DEFAULT_SIZES, "1000,10000,100000")
        self.assertEqual(args.sizes, DEFAULT_SIZES)
        self.assertEqual(args.systems, DEFAULT_SYSTEMS)
        self.assertIn("agentmem", args.systems)
        for legacy in ("memos", "mempalace"):
            self.assertNotIn(legacy, args.systems)

    def test_empty_sizes_keeps_single_size_fallback(self):
        args = build_parser().parse_args(["--sizes", "", "--size", "25"])
        self.assertEqual(args.sizes, "")
        self.assertEqual(args.size, 25)


if __name__ == "__main__":
    unittest.main()
