#!/usr/bin/env python3
"""Exercise the shell suite's actual usage checker with tiny Git inventories."""
import os
from pathlib import Path
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
SUITE = ROOT / "scripts/ci/test-review-split-wave.sh"
SCRIPT = ROOT / "scripts/ci/review-split-wave.sh"
CEILINGS = ROOT / "scripts/ci/lib/resource_ceilings.py"


class UsageInventoryTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(prefix="assay-usage-test-")
        self.addCleanup(self.tmp.cleanup)
        self.repo = Path(self.tmp.name) / "repo"
        template = Path(self.tmp.name) / "empty-template"
        template.mkdir()
        self.env = {k: v for k, v in os.environ.items() if not k.startswith("GIT_")}
        self.env.update(
            PYTHONDONTWRITEBYTECODE="1",
            PYTHONPATH=str(CEILINGS.parent),
            BOUNDED_INVENTORY_MAX_BYTES="512",
            GIT_CONFIG_GLOBAL=os.devnull,
            GIT_CONFIG_SYSTEM=os.devnull,
            GIT_CONFIG_NOSYSTEM="1",
            GIT_ATTR_NOSYSTEM="1",
        )
        subprocess.run(["git", "init", "-q", "--template=" + str(template),
                        str(self.repo)], check=True,
                       env=self.env, timeout=10, capture_output=True)
        for key in ("core.attributesFile", "core.excludesFile", "core.hooksPath"):
            subprocess.run(["git", "-C", str(self.repo), "config", key, os.devnull],
                           check=True, env=self.env, timeout=10, capture_output=True)
        self.add_path("crates/assay-sim/src/attacks/consumer_downgrade.rs")
        self.add_path("crates/assay-registry/src/trust.rs")

    def add_path(self, name):
        path = self.repo / name
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text("fixture\n")
        subprocess.run(["git", "-C", str(self.repo), "add", "--", name],
                       check=True, env=self.env, timeout=10, capture_output=True)

    def check_usage(self, root=None):
        # Execute the existing function verbatim: no copied regex or inventory rule.
        text = SUITE.read_text()
        start = text.index("assert_usage_examples_exist() {")
        end = text.index("\nmake_stub_cargo() {", start)
        command = ('set -euo pipefail\nROOT="$1"\nCEILINGS="$2"\n'
                   + text[start:end] + '\nassert_usage_examples_exist "$3"\n')
        return subprocess.run(
            ["bash", "-c", command, "usage-test", str(root or self.repo),
             str(CEILINGS), str(SCRIPT)],
            env=self.env, capture_output=True, text=True, timeout=10,
        )

    def test_unrelated_inventory_over_cap_keeps_matching_examples(self):
        # Removing Git pathspecs must make this assertion fail on byte overflow.
        for i in range(8):
            self.add_path("unrelated/" + str(i) + "x" * 80)
        result = self.check_usage()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("2 usage example regexes match tracked files", result.stdout)

    def test_missing_example_match_refuses(self):
        # Skipping regex checks, or accepting any scoped file, must fail this case.
        subprocess.run(
            ["git", "-C", str(self.repo), "rm", "-q", "--cached", "--",
             "crates/assay-registry/src/trust.rs"],
            check=True, env=self.env, timeout=10, capture_output=True,
        )
        result = self.check_usage()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("usage example regex matches no tracked file", result.stderr)

    def test_scoped_inventory_over_cap_refuses(self):
        # Bypassing the canonical helper must make this assertion fail.
        for i in range(8):
            self.add_path("crates/assay-sim/" + str(i) + "x" * 80)
        result = self.check_usage()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("inventory exceeds max byte budget 512", result.stderr)

    def test_git_producer_failure_refuses(self):
        # Ignoring the producer status must lose this diagnostic.
        with tempfile.TemporaryDirectory(prefix="assay-usage-not-git-") as other:
            result = self.check_usage(Path(other))
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("git ls-files failed", result.stderr)


if __name__ == "__main__":
    unittest.main()
