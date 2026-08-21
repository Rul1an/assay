#!/usr/bin/env python3
"""#206: one opt-in pma-v0-repro registry row. Mutations are copies, not edits.

    python3 -W error::ResourceWarning conformance/tests/test_pma_v0_registration.py

A digest addresses bytes. This test does not pull, run, or endorse the image.
"""

from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance"))

import implementations

PACKAGING_COMMIT = "c226a34f3cea50a114607c35f0976048ff3cab2b"
FREEZE_COMMIT = "fc6cb25d5d1e18396d78815723e241425447ab7d"
IMAGE = (
    "ghcr.io/rul1an/pma-v0-repro@sha256:"
    "88a5ef285a80dc0caeb2b11093eba79f8f08c870b549cafc395aa77ee5ffc493"
)
SOURCE = "https://github.com/Rul1an/pma-v0-repro"
CONSENT_URL = (
    "https://github.com/Rul1an/pma-v0-repro/blob/"
    + PACKAGING_COMMIT
    + "/README.md"
)
ROW_ID = "pma-v0-repro"


def _load():
    return implementations.load_implementations()


def _row(test: unittest.TestCase) -> dict:
    doc = _load()
    rows = doc["implementations"]
    test.assertEqual(len(rows), 1, "exactly one implementation row is registered")
    return rows[0]


def _write_and_load(doc: dict) -> None:
    with tempfile.TemporaryDirectory() as raw:
        path = Path(raw) / "implementations.json"
        path.write_text(json.dumps(doc), encoding="utf-8")
        implementations.load_implementations(path)


class CheckedInRow(unittest.TestCase):
    def test_schema_valid_pma_v0_repro_row(self) -> None:
        doc = _load()
        self.assertEqual(doc["schema"], implementations.SCHEMA)
        row = _row(self)
        self.assertEqual(row["id"], ROW_ID)
        self.assertEqual(row["suite"], "privileged-mcp-action-v0")
        self.assertEqual(row["source"], SOURCE)
        self.assertEqual(row["commit"], PACKAGING_COMMIT)
        self.assertEqual(row["image"], IMAGE)
        self.assertEqual(row["language"], "python")
        self.assertEqual(row["reproduction_mode"], "other_disclosed")
        self.assertNotEqual(row["commit"], FREEZE_COMMIT)

    def test_authorship_is_agent_assisted_with_method(self) -> None:
        row = _row(self)
        auth = row["authorship"]
        self.assertEqual(auth["kind"], "agent-assisted")
        self.assertTrue(auth["model"].strip())
        strategy = auth["prompt_strategy"]
        self.assertTrue(strategy.strip())
        self.assertIn(FREEZE_COMMIT, strategy)
        self.assertIn("other_disclosed", strategy)
        self.assertIn("blind_from_spec", strategy)
        self.assertIn(CONSENT_URL, strategy)

    def test_consent_url_is_the_pinned_readme(self) -> None:
        row = _row(self)
        self.assertIn(CONSENT_URL, row["authorship"]["prompt_strategy"])
        self.assertIn(PACKAGING_COMMIT, CONSENT_URL)

    def test_non_claim_strings_are_not_inverted(self) -> None:
        row = _row(self)
        blob = json.dumps(row)
        for forbidden in (
            "independent implementation",
            "publisher authenticated",
            "malware safe",
            "docker is a sandbox",
            "certified conformant",
        ):
            self.assertNotIn(forbidden, blob.lower())


class Mutations(unittest.TestCase):
    def _doc(self) -> dict:
        return copy.deepcopy(_load())

    def test_digest_swap_is_rejected(self) -> None:
        row = _row(self)
        swapped = "ghcr.io/rul1an/pma-v0-repro@sha256:" + "0" * 64
        self.assertEqual(row["image"], IMAGE)
        self.assertNotEqual(row["image"], swapped)
        self.assertNotIn("d81847d77b5add3ae68eb1a640ee1fc9cba854911fb17161a98f4134a92a0043", row["image"])

    def test_source_commit_swap_is_rejected(self) -> None:
        row = _row(self)
        self.assertNotEqual(row["commit"], FREEZE_COMMIT)
        self.assertNotEqual(row["commit"], "25939fd80baf9ee7a179f3b4aeb904ed6d243b30")
        self.assertEqual(row["commit"], PACKAGING_COMMIT)

    def test_tag_only_image_is_rejected_by_validator(self) -> None:
        doc = self._doc()
        doc["implementations"][0]["image"] = "ghcr.io/rul1an/pma-v0-repro:c226a34f"
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            _write_and_load(doc)
        self.assertIn("image", str(ctx.exception).lower())

    def test_missing_prompt_strategy_is_rejected(self) -> None:
        doc = self._doc()
        del doc["implementations"][0]["authorship"]["prompt_strategy"]
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            _write_and_load(doc)
        self.assertIn("prompt_strategy", str(ctx.exception).lower())

    def test_missing_model_is_rejected(self) -> None:
        doc = self._doc()
        del doc["implementations"][0]["authorship"]["model"]
        with self.assertRaises(implementations.ImplementationRegistryError) as ctx:
            _write_and_load(doc)
        self.assertIn("model", str(ctx.exception).lower())

    def test_human_authorship_is_rejected_for_this_row(self) -> None:
        row = _row(self)
        self.assertEqual(row["authorship"]["kind"], "agent-assisted")
        self.assertNotEqual(row["authorship"]["kind"], "human")


if __name__ == "__main__":
    unittest.main(verbosity=2)
