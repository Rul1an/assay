#!/usr/bin/env python3
"""Stdlib unittest suite for the coverage-claims gate showcase.

Run: python3 -m unittest discover -s examples/coverage-claims-gate -p 'test_*.py'
"""

from __future__ import annotations

import json
import os
import unittest

import check_claims as cc

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures")


def _load(name: str) -> dict:
    with open(os.path.join(FIXTURES, name), "r", encoding="utf-8") as handle:
        return json.load(handle)


class ParseClaimSpecTests(unittest.TestCase):
    def test_valid_spec(self) -> None:
        self.assertEqual(
            cc.parse_claim_spec("positive:network_endpoints"),
            ("positive", "network_endpoints"),
        )

    def test_strips_whitespace(self) -> None:
        self.assertEqual(
            cc.parse_claim_spec("  exhaustive : filesystem_paths_touched "),
            ("exhaustive", "filesystem_paths_touched"),
        )

    def test_missing_colon(self) -> None:
        with self.assertRaises(ValueError):
            cc.parse_claim_spec("positive")

    def test_unknown_type(self) -> None:
        with self.assertRaises(ValueError):
            cc.parse_claim_spec("definitely:filesystem_paths_touched")

    def test_empty_dimension(self) -> None:
        with self.assertRaises(ValueError):
            cc.parse_claim_spec("positive:")


class EvaluateClaimTests(unittest.TestCase):
    def setUp(self) -> None:
        self.annotation = _load("annotation.json")

    def test_positive_partial_permitted(self) -> None:
        permitted, _ = cc.evaluate_claim(
            self.annotation, "positive", "filesystem_paths_touched"
        )
        self.assertTrue(permitted)

    def test_positive_absent_blocked(self) -> None:
        permitted, detail = cc.evaluate_claim(
            self.annotation, "positive", "network_endpoints"
        )
        self.assertFalse(permitted)
        self.assertIn("absent", detail)

    def test_positive_missing_cell_blocked(self) -> None:
        permitted, detail = cc.evaluate_claim(
            self.annotation, "positive", "process_execs"
        )
        self.assertFalse(permitted)
        self.assertIn("nothing observed", detail)

    def test_exhaustive_weak_blocked(self) -> None:
        permitted, detail = cc.evaluate_claim(
            self.annotation, "exhaustive", "filesystem_paths_touched"
        )
        self.assertFalse(permitted)
        self.assertIn("weak", detail)

    def test_bounded_negative_blocked_by_descriptor(self) -> None:
        permitted, detail = cc.evaluate_claim(
            self.annotation, "bounded_negative", "filesystem_paths_touched"
        )
        self.assertFalse(permitted)
        self.assertIn("blocked", detail)

    def test_bounded_negative_not_evaluable_for_reported_dimension(self) -> None:
        # tool_calls is a reported dimension, not a measured one -> not evaluable.
        permitted, detail = cc.evaluate_claim(
            self.annotation, "bounded_negative", "tool_calls"
        )
        self.assertFalse(permitted)
        self.assertIn("non-measured", detail)

    def test_bounded_negative_permitted_when_measured_and_unblocked(self) -> None:
        # A measured dimension with no matching blocked_claims entry -> permitted.
        permitted, _ = cc.evaluate_claim(
            self.annotation, "bounded_negative", "network_endpoints"
        )
        self.assertTrue(permitted)


class GateTests(unittest.TestCase):
    def setUp(self) -> None:
        self.annotation = _load("annotation.json")

    def test_pass_policy(self) -> None:
        report = cc.gate(self.annotation, ["positive:filesystem_paths_touched"])
        self.assertTrue(report["passed"])
        self.assertEqual(report, _load("expected_pass.json"))

    def test_blocked_policy(self) -> None:
        specs = [
            "positive:filesystem_paths_touched",
            "positive:network_endpoints",
            "exhaustive:filesystem_paths_touched",
            "bounded_negative:filesystem_paths_touched",
        ]
        report = cc.gate(self.annotation, specs)
        self.assertFalse(report["passed"])
        self.assertEqual(report, _load("expected_blocked.json"))

    def test_wrong_schema_rejected(self) -> None:
        with self.assertRaises(ValueError):
            cc.gate({"schema": "something.else.v0"}, ["positive:x"])

    def test_single_block_fails_whole_gate(self) -> None:
        report = cc.gate(
            self.annotation,
            ["positive:filesystem_paths_touched", "positive:network_endpoints"],
        )
        self.assertFalse(report["passed"])
        self.assertTrue(report["results"][0]["permitted"])
        self.assertFalse(report["results"][1]["permitted"])


class RenderTextTests(unittest.TestCase):
    def test_pass_render(self) -> None:
        report = {
            "passed": True,
            "results": [
                {"claim": "positive:fs", "permitted": True, "detail": "ok"}
            ],
        }
        out = cc.render_text(report)
        self.assertIn("[PERMIT] positive:fs: ok", out)
        self.assertTrue(out.rstrip().endswith("PASS"))

    def test_fail_render(self) -> None:
        report = {
            "passed": False,
            "results": [
                {"claim": "positive:net", "permitted": False, "detail": "absent"}
            ],
        }
        out = cc.render_text(report)
        self.assertIn("[BLOCK ] positive:net: absent", out)
        self.assertTrue(out.rstrip().endswith("FAIL"))


if __name__ == "__main__":
    unittest.main()
