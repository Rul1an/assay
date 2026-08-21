#!/usr/bin/env python3
"""Mutations for the external Conformance inventory callsite pin.

    python3 scripts/ci/test-conformance-inventory-callsite.py

The inventory tests already encode this contract, but they execute inside the
step they guard. These mutations therefore target the later required-CI
checker, not the in-step suite. The live workflow file is never written.
"""

from __future__ import annotations

import hashlib
import importlib.util
import sys
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import (  # noqa: E402
    INVENTORY_RUN_SCRIPT,
    REQUIRED_RUN_ALL,
    named_step,
)

CHECKER_PATH = REPO / "scripts/ci/check-conformance-inventory-callsite.py"
B1_PATH = REPO / "scripts/ci/test-ci-hardening-b1.sh"
CI_YML = REPO / ".github/workflows/ci.yml"
JOB = "scope"
STEP = "Conformance inventory"
CHECKER_PATH_TOKEN = "scripts/ci/check-conformance-inventory-callsite.py"
TEST_PATH_TOKEN = "scripts/ci/test-conformance-inventory-callsite.py"


def load_checker():
    if not CHECKER_PATH.is_file():
        raise AssertionError(f"missing checker {CHECKER_PATH}")
    spec = importlib.util.spec_from_file_location(
        "conformance_inventory_callsite", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("checker is not loadable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    checker = getattr(module, "conformance_inventory_callsite_problems", None)
    if not callable(checker):
        raise AssertionError("conformance_inventory_callsite_problems missing")
    return checker


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def delete_step(text: str) -> str:
    step = named_step(text, JOB, STEP)
    mutated = text.replace(step, "", 1)
    if mutated == text or f"      - name: {STEP}\n" in mutated:
        raise AssertionError("delete-step mutation was a no-op")
    return mutated


def rename_step(text: str) -> str:
    mutated = text.replace(
        f"      - name: {STEP}\n",
        "      - name: Inventory conformance\n",
        1,
    )
    if mutated == text:
        raise AssertionError("rename mutation was a no-op")
    return mutated


def false_if(text: str) -> str:
    needle = f"      - name: {STEP}\n"
    mutated = text.replace(needle, needle + "        if: false\n", 1)
    if mutated == text:
        raise AssertionError("if:false mutation was a no-op")
    return mutated


def comment_live_commands(text: str) -> str:
    mutated = text
    for command in INVENTORY_RUN_SCRIPT[1:]:
        needle = f"          {command}\n"
        if needle not in mutated:
            raise AssertionError(f"missing live command {command!r}")
        mutated = mutated.replace(needle, f"          # {command}\n", 1)
    if mutated == text:
        raise AssertionError("comment mutation was a no-op")
    return mutated


def colon_one_command(text: str) -> str:
    needle = f"          {REQUIRED_RUN_ALL}\n"
    mutated = text.replace(needle, "          :\n", 1)
    if mutated == text:
        raise AssertionError("colon mutation was a no-op")
    return mutated


def reorder_one_command(text: str) -> str:
    first = "          python3 conformance/registry.py\n"
    second = "          python3 conformance/implementations.py\n"
    if first not in text or second not in text:
        raise AssertionError("reorder targets missing")
    mutated = text.replace(first, "__SWAP_A__\n", 1)
    mutated = mutated.replace(second, first, 1)
    mutated = mutated.replace("__SWAP_A__\n", second, 1)
    if mutated == text:
        raise AssertionError("reorder mutation was a no-op")
    return mutated


MUTATIONS = (
    ("delete_step", delete_step),
    ("rename_step", rename_step),
    ("false_if", false_if),
    ("comment_live_commands", comment_live_commands),
    ("colon_one_command", colon_one_command),
    ("reorder_one_command", reorder_one_command),
)


class ConformanceInventoryCallsite(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.live = CI_YML.read_text(encoding="utf-8")
        cls.live_digest = sha256_text(cls.live)
        cls.problems_fn = staticmethod(load_checker())

    def tearDown(self) -> None:
        current = CI_YML.read_text(encoding="utf-8")
        self.assertEqual(sha256_text(current), self.live_digest)
        self.assertEqual(current, self.live)

    def test_pristine_workflow_is_green(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])

    def test_each_mutation_fails_independently(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        for label, mutate in MUTATIONS:
            with self.subTest(label=label):
                mutated = mutate(self.live)
                self.assertNotEqual(mutated, self.live)
                problems = self.problems_fn(mutated)
                self.assertTrue(
                    problems,
                    f"{label} must fail the later required-CI checker",
                )
        self.assertEqual(self.problems_fn(self.live), [])

    def test_b1_actively_invokes_checker_and_mutations(self) -> None:
        text = B1_PATH.read_text(encoding="utf-8")
        active = [
            line
            for line in text.splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        ]
        joined = "\n".join(active)
        self.assertIn(CHECKER_PATH_TOKEN, joined)
        self.assertIn(TEST_PATH_TOKEN, joined)
        self.assertTrue(
            any("python3" in line and CHECKER_PATH_TOKEN in line for line in active),
            "B1 must actively invoke the live callsite checker",
        )
        self.assertTrue(
            any("python3" in line and TEST_PATH_TOKEN in line for line in active),
            "B1 must actively invoke the callsite mutation tests",
        )


if __name__ == "__main__":
    unittest.main()
