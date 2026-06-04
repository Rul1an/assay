#!/usr/bin/env python3
"""Stdlib unittest suite for the coverage fleet summary showcase.

Run: python3 -m unittest discover -s examples/coverage-fleet-summary -p 'test_*.py'
"""

from __future__ import annotations

import json
import os
import unittest

import aggregate_coverage as ac

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures")
RUNS = os.path.join(FIXTURES, "runs")


def _load(path: str) -> dict:
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def _all_runs() -> list:
    return [
        _load(os.path.join(RUNS, name))
        for name in sorted(os.listdir(RUNS))
        if name.endswith(".json")
    ]


class WeakerTests(unittest.TestCase):
    def test_orders_strengths(self) -> None:
        self.assertEqual(ac._weaker("strong", "absent"), "absent")
        self.assertEqual(ac._weaker("partial", "weak"), "weak")
        self.assertEqual(ac._weaker("partial", "strong"), "partial")

    def test_unknown_is_weakest(self) -> None:
        self.assertEqual(ac._weaker("strong", "bogus"), "bogus")


class FoldTests(unittest.TestCase):
    def setUp(self) -> None:
        self.summary = ac.fold(_all_runs())

    def test_matches_expected_fixture(self) -> None:
        self.assertEqual(self.summary, _load(os.path.join(FIXTURES, "expected_summary.json")))

    def test_run_count(self) -> None:
        self.assertEqual(self.summary["run_count"], 3)

    def test_filesystem_floor_is_weakest_observed(self) -> None:
        fs = self.summary["dimensions"]["filesystem_paths_touched"]
        # strengths seen: partial, strong, absent -> floor is absent.
        self.assertEqual(fs["fleet_positive_floor"], "absent")
        self.assertEqual(fs["measured_positive"]["strong"], 1)
        self.assertEqual(fs["measured_positive"]["absent"], 1)

    def test_unobserved_dimension_has_no_floor(self) -> None:
        ke = self.summary["dimensions"]["kernel_file_operations"]
        self.assertEqual(ke["runs_observed"], 0)
        self.assertEqual(ke["fleet_positive_floor"], "missing")
        self.assertEqual(ke["measured_positive"]["missing"], 3)

    def test_bounded_negative_block_counts(self) -> None:
        dims = self.summary["dimensions"]
        self.assertEqual(dims["filesystem_paths_touched"]["bounded_negative_blocked"], 2)
        self.assertEqual(dims["network_endpoints"]["bounded_negative_blocked"], 2)

    def test_exhaustive_distribution(self) -> None:
        exh = self.summary["dimensions"]["filesystem_paths_touched"]["exhaustive_equality"]
        # run-01 weak, run-02 partial, run-03 missing.
        self.assertEqual(exh["weak"], 1)
        self.assertEqual(exh["partial"], 1)
        self.assertEqual(exh["missing"], 1)

    def test_empty_fleet(self) -> None:
        summary = ac.fold([])
        self.assertEqual(summary["run_count"], 0)
        for entry in summary["dimensions"].values():
            self.assertEqual(entry["runs_observed"], 0)
            self.assertEqual(entry["fleet_positive_floor"], "missing")

    def test_wrong_schema_rejected(self) -> None:
        with self.assertRaises(ValueError):
            ac.fold([{"schema": "something.else.v0"}])


class RenderTextTests(unittest.TestCase):
    def test_renders_each_dimension(self) -> None:
        out = ac.render_text(ac.fold(_all_runs()))
        self.assertIn("filesystem_paths_touched:", out)
        self.assertIn("positive floor: absent", out)
        self.assertIn("bounded-negative blocked in 2 run(s)", out)


if __name__ == "__main__":
    unittest.main()
