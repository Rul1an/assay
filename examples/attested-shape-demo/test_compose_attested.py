#!/usr/bin/env python3
"""Stdlib unittest suite for the attested-shape synthetic demonstrator.

Run: python3 -m unittest discover -s examples/attested-shape-demo -p 'test_*.py'

Reminder: the subject under test is a SYNTHETIC DEMONSTRATOR. These tests assert
the composition SHAPE, not any real attestation behaviour.
"""

from __future__ import annotations

import json
import os
import unittest

import compose_attested as ca

HERE = os.path.dirname(os.path.abspath(__file__))
FIXTURES = os.path.join(HERE, "fixtures")


def _load(name: str) -> dict:
    with open(os.path.join(FIXTURES, name), "r", encoding="utf-8") as handle:
        return json.load(handle)


class AtMostTests(unittest.TestCase):
    def test_caps_to_ceiling(self) -> None:
        self.assertEqual(ca._at_most("strong", "weak"), "weak")

    def test_keeps_lower(self) -> None:
        self.assertEqual(ca._at_most("absent", "strong"), "absent")

    def test_equal(self) -> None:
        self.assertEqual(ca._at_most("partial", "partial"), "partial")


class ComposeTests(unittest.TestCase):
    def setUp(self) -> None:
        self.cell = _load("cell.json")

    def test_no_envelope_degrades(self) -> None:
        report = ca.compose(self.cell, None)
        self.assertEqual(report["effective_strength"], "weak")
        self.assertEqual(report["effective_basis"], "inferred")

    def test_verified_matching_subject_stands(self) -> None:
        report = ca.compose(self.cell, _load("envelope_verified.json"))
        self.assertEqual(report["effective_strength"], "strong")
        self.assertEqual(report["effective_basis"], "attested")

    def test_verified_wrong_subject_degrades(self) -> None:
        report = ca.compose(self.cell, _load("envelope_wrong_subject.json"))
        self.assertEqual(report["effective_strength"], "weak")
        self.assertEqual(report["effective_basis"], "inferred")
        self.assertIn("does not cover this subject", report["reason"])

    def test_unverified_envelope_degrades(self) -> None:
        report = ca.compose(self.cell, _load("envelope_unverified.json"))
        self.assertEqual(report["effective_strength"], "weak")
        self.assertEqual(report["effective_basis"], "inferred")
        self.assertIn("not evidence", report["reason"])

    def test_declared_weak_never_upgraded(self) -> None:
        # A verified, matching envelope must not upgrade a cell that only
        # declared a weak claim — composition can cap but never inflate.
        weak_cell = dict(self.cell, claim_strength="weak")
        report = ca.compose(weak_cell, _load("envelope_verified.json"))
        self.assertEqual(report["effective_strength"], "weak")

    def test_report_always_carries_demo_banner(self) -> None:
        for env in (None, _load("envelope_verified.json")):
            report = ca.compose(self.cell, env)
            self.assertEqual(report["demo"], ca.DEMO_BANNER)


class RenderTests(unittest.TestCase):
    def test_text_render_includes_banner(self) -> None:
        out = ca.render_text(ca.compose(_load("cell.json"), None))
        self.assertIn(ca.DEMO_BANNER, out)
        self.assertIn("effective:", out)


if __name__ == "__main__":
    unittest.main()
