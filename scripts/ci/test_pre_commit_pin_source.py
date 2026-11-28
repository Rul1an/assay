#!/usr/bin/env python3
"""pre-commit is installed at one pinned version, read from one place.

The pin lives in .github/workflows/kernel-matrix.yml (`pip install pre-commit==X`), where it
already was. scripts/ci/read-pre-commit-pin.sh is the only reader: both CI steps that install
pre-commit and the contributor setup ask it, so no copy of the number can drift.
`minimum_pre_commit_version` in .pre-commit-config.yaml is a minimum, not this pin, and is not
read here.
"""
from __future__ import annotations

import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
READER = "scripts/ci/read-pre-commit-pin.sh"
SOURCE = ".github/workflows/kernel-matrix.yml"
CI = ".github/workflows/ci.yml"
INSTALLING_STEPS = ("Evidence vocabulary guard", "Editor-release hook pre-commit consumer")
PIN_LINE = 'pin="$(bash scripts/ci/read-pre-commit-pin.sh)"'
LITERAL_PIN = re.compile(r"pre-commit==[0-9]")


def run_reader(tree: Path) -> subprocess.CompletedProcess:
    return subprocess.run(["bash", str(tree / READER)], cwd=tree, capture_output=True, text=True,
                          timeout=30)


def workflow_steps(path: Path) -> list[dict]:
    raw = subprocess.check_output(
        ["ruby", "-rjson", "-ryaml", "-e",
         "puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))"],
        input=path.read_bytes(),
    )
    data = json.loads(raw)
    return [step for job in data["jobs"].values() for step in job.get("steps", [])]


class Reader(unittest.TestCase):
    def test_prints_exactly_the_kernel_matrix_pin(self) -> None:
        pins = set(re.findall(r"pip install pre-commit==([0-9.]+)", (ROOT / SOURCE).read_text()))
        self.assertEqual(len(pins), 1, pins)
        result = run_reader(ROOT)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, pins.pop() + "\n")

    def test_refuses_a_missing_or_ambiguous_pin(self) -> None:
        source = (ROOT / SOURCE).read_text()
        for name, text in (
                ("no pin", re.sub(r"pip install pre-commit==[0-9.]+", "pip install pre-commit", source)),
                ("two pins", source + "\n# pip install pre-commit==0.0.1\n")):
            with self.subTest(name), tempfile.TemporaryDirectory() as temporary:
                tree = Path(temporary)
                (tree / SOURCE).parent.mkdir(parents=True)
                (tree / SOURCE).write_text(text)
                (tree / READER).parent.mkdir(parents=True)
                shutil.copy2(ROOT / READER, tree / READER)
                result = run_reader(tree)
                self.assertNotEqual(result.returncode, 0, result.stdout)
                self.assertEqual(result.stdout, "", "a refused pin must print no version")
                self.assertIn("pre-commit", result.stderr)


class Callers(unittest.TestCase):
    def test_both_ci_steps_that_install_pre_commit_ask_the_reader(self) -> None:
        steps = {step.get("name"): step for step in workflow_steps(ROOT / CI)}
        for name in INSTALLING_STEPS:
            with self.subTest(step=name):
                self.assertIn(name, steps)
                run = steps[name]["run"]
                lines = [line.strip() for line in run.splitlines()]
                self.assertIn(PIN_LINE, lines)
                installs = [line for line in lines if "pip install" in line and "pre-commit" in line]
                self.assertEqual(len(installs), 1, installs)
                self.assertIn('"pre-commit==${pin}"', installs[0])
                self.assertLess(lines.index(PIN_LINE), lines.index(installs[0]))
                self.assertIsNone(LITERAL_PIN.search(run), "a literal pin is a second copy")

    def test_no_other_literal_pin_in_ci_the_setup_docs_or_the_consumer_contracts(self) -> None:
        # The consumer contracts name the pin in their "pre-commit not on PATH" hint; a fixed number
        # there is a copy that a bump would leave behind.
        for path in (CI, "CONTRIBUTING.md", "docs/DEVELOPER_HANDOFF.md",
                     "scripts/ci/test-editor-release-hook-precommit-consumer.sh",
                     "scripts/ci/test-linux-check-precommit-consumer.sh"):
            with self.subTest(path=path):
                self.assertIsNone(LITERAL_PIN.search((ROOT / path).read_text()))

    def test_the_setup_docs_install_the_pinned_version(self) -> None:
        for path in ("CONTRIBUTING.md", "docs/DEVELOPER_HANDOFF.md"):
            with self.subTest(path=path):
                text = (ROOT / path).read_text()
                self.assertIn('pre-commit==$(bash scripts/ci/read-pre-commit-pin.sh)', text)
                self.assertNotRegex(text, r"(?m)^\s*pip install pre-commit\s*$")


if __name__ == "__main__":
    unittest.main()
