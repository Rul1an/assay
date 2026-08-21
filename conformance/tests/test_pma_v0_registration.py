#!/usr/bin/env python3
"""#206: one opt-in pma-v0-repro registry row, plus PMA delivery wiring.

    python3 -W error::ResourceWarning conformance/tests/test_pma_v0_registration.py

A digest addresses bytes. This test does not pull, run, or endorse the image.
Generic schema rejection lives in test_implementations.py.
Required CI also pins that privileged-mcp-action-conformance.yml still
invokes both kit files in one unittest command.
"""

from __future__ import annotations

import json
import re
import sys
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
MODEL = "Grok Bot"
CI_YML = REPO / ".github/workflows/ci.yml"
CONFORMANCE_WORKFLOW = REPO / ".github/workflows/privileged-mcp-action-conformance.yml"
CI_COMMAND = (
    "python3 -W error::ResourceWarning "
    "conformance/tests/test_pma_v0_registration.py"
)
ACTIVATION_KIT = (
    "conformance/privileged-mcp-action-v0/tests/test_activation_kit.py"
)
EXECUTOR_KIT = (
    "conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py"
)
COMBINED_UNITTEST = (
    "          python3 -m unittest \\\n"
    f"            {ACTIVATION_KIT} \\\n"
    f"            {EXECUTOR_KIT}"
)
COMBINED_UNITTEST_RE = re.compile(
    r"python3 -m unittest \\\n"
    rf"\s+{re.escape(ACTIVATION_KIT)} \\\n"
    rf"\s+{re.escape(EXECUTOR_KIT)}"
)
INVENTORY_STEP_RE = re.compile(
    r"(?ms)^      - name: Conformance inventory\n(?:        .+\n)+"
)
JOB_KEY_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:")


def _inventory_step() -> str:
    text = CI_YML.read_text(encoding="utf-8")
    lines = text.splitlines(keepends=True)
    start = next((i for i, line in enumerate(lines) if line.startswith("  scope:")), None)
    if start is None:
        raise AssertionError("ci.yml missing scope job")
    end = start + 1
    while end < len(lines) and not JOB_KEY_RE.match(lines[end]):
        end += 1
    step = INVENTORY_STEP_RE.search("".join(lines[start:end]))
    if step is None:
        raise AssertionError("scope job missing Conformance inventory step")
    return step.group(0)


def _row(test: unittest.TestCase) -> dict:
    doc = implementations.load_implementations()
    rows = doc["implementations"]
    test.assertEqual(len(rows), 1, "exactly one implementation row is registered")
    return rows[0]


class CheckedInRow(unittest.TestCase):
    def test_row_pins_measured_facts(self) -> None:
        doc = implementations.load_implementations()
        self.assertEqual(doc["schema"], implementations.SCHEMA)
        row = _row(self)
        self.assertEqual(row["id"], ROW_ID)
        self.assertEqual(row["suite"], "privileged-mcp-action-v0")
        self.assertEqual(row["source"], SOURCE)
        self.assertEqual(row["commit"], PACKAGING_COMMIT)
        self.assertNotEqual(row["commit"], FREEZE_COMMIT)
        self.assertEqual(row["image"], IMAGE)
        self.assertEqual(row["language"], "python")
        self.assertEqual(row["reproduction_mode"], "other_disclosed")
        auth = row["authorship"]
        self.assertEqual(auth["kind"], "agent-assisted")
        self.assertEqual(auth["model"], MODEL)
        strategy = auth["prompt_strategy"]
        self.assertIn(FREEZE_COMMIT, strategy)
        self.assertIn("other_disclosed", strategy)
        self.assertIn("blind_from_spec", strategy)
        self.assertIn(CONSENT_URL, strategy)
        blob = json.dumps(row).lower()
        for forbidden in (
            "independent implementation",
            "publisher authenticated",
            "malware safe",
            "docker is a sandbox",
            "certified conformant",
        ):
            self.assertNotIn(forbidden, blob)


class RequiredCi(unittest.TestCase):
    def test_conformance_inventory_invokes_this_module(self) -> None:
        step = _inventory_step()
        self.assertIn(CI_COMMAND, step)
        self.assertNotIn("continue-on-error", step)


class PmaDeliveryWiring(unittest.TestCase):
    """Required-CI pin of PMA delivery: both kit files in one unittest command."""

    def _assert_combined_command(self, text: str) -> None:
        self.assertIn("test_activation_kit.py", text)
        self.assertIn("test_oci_candidate_executor.py", text)
        self.assertRegex(text, COMBINED_UNITTEST_RE)

    def test_conformance_workflow_runs_both_kit_files(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        self._assert_combined_command(text)

    def test_replacing_combined_command_with_true_is_rejected(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn(COMBINED_UNITTEST, text)
        mutated = text.replace(COMBINED_UNITTEST, "          true", 1)
        with self.assertRaises(AssertionError):
            self._assert_combined_command(mutated)

    def test_removing_only_activation_arg_is_rejected(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        mutated = text.replace(f"            {ACTIVATION_KIT} \\\n", "", 1)
        with self.assertRaises(AssertionError):
            self._assert_combined_command(mutated)

    def test_removing_only_executor_arg_is_rejected(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        mutated = text.replace(f"            {EXECUTOR_KIT}\n", "", 1)
        with self.assertRaises(AssertionError):
            self._assert_combined_command(mutated)

    def test_reverse_and_noop_remain_green(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        self._assert_combined_command(text)
        mutated = text.replace(COMBINED_UNITTEST, "          true", 1)
        self.assertNotEqual(mutated, text)
        with self.assertRaises(AssertionError):
            self._assert_combined_command(mutated)
        self._assert_combined_command(text)


if __name__ == "__main__":
    unittest.main(verbosity=2)
