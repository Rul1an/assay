#!/usr/bin/env python3
"""The golden-path contract must remove its own early scratch on failure (H10).

Exercise the real entry point through its command-resolution controls, then stop at a malformed
manifest before the rest of the battery. No release download, build or model runs here.
"""
from __future__ import annotations

import os
import subprocess
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CONTRACT = ROOT / "scripts/ci/test-published-release-golden-path-contract.sh"
DRIVER = ROOT / "scripts/ci/published-release-golden-path.sh"


class ScratchCleanupTest(unittest.TestCase):
    def check_failure(self, driver: str, diagnostic: str) -> None:
        with tempfile.TemporaryDirectory(prefix="assay-contract-cleanup-") as name:
            root = Path(name)
            tmp = root / "temporary files"
            tmp.mkdir()
            # Other sessions' recovery files and directories must survive the contract's cleanup.
            (tmp / "keep.patch").write_bytes(b"unrelated recovery bytes\n")
            (tmp / "other-session").mkdir()
            (tmp / "other-session" / "keep.log").write_bytes(b"unrelated evidence\n")
            source = root / "driver.sh"
            source.write_text(driver)
            manifest = root / "invalid-manifest.json"
            manifest.write_text("{")
            env = {k: v for k, v in os.environ.items()
                   if not k.startswith(("GIT_", "PRE_COMMIT"))}
            env.update(TMPDIR=str(tmp), DRIVER=str(source), MANIFEST=str(manifest),
                       PYTHONDONTWRITEBYTECODE="1")
            result = subprocess.run(["bash", str(CONTRACT)], cwd=ROOT, env=env,
                                    capture_output=True, text=True, timeout=30)
            self.assertEqual(result.returncode, 1, result.stdout + result.stderr)
            self.assertIn(diagnostic, result.stderr, result.stdout + result.stderr)
            self.assertEqual(sorted(p.name for p in tmp.iterdir()),
                             ["keep.patch", "other-session"], "contract leaked owned scratch")
            self.assertEqual((tmp / "keep.patch").read_bytes(), b"unrelated recovery bytes\n")
            self.assertEqual((tmp / "other-session" / "keep.log").read_bytes(),
                             b"unrelated evidence\n")
            self.assertEqual(source.read_text(), driver, "contract changed its input")

    def test_failed_initial_resolution_cleans_scratch(self) -> None:
        driver = DRIVER.read_text().replace("resolve_required_command() {", "renamed_resolver() {")
        self.check_failure(driver, "resolve_required_command function is missing")

    def test_failed_mutation_setup_cleans_both_early_allocations(self) -> None:
        # Same shell behavior, but the deliberate mutation anchor is absent. The real positive
        # resolution control must finish before mutation setup refuses this fixture.
        driver = DRIVER.read_text().replace("CDPATH='' cd -P --", 'CDPATH="" cd -P --')
        self.check_failure(driver, "CDPATH guard must occur exactly once")

    def test_successful_early_controls_clean_up_on_later_failure(self) -> None:
        self.check_failure(DRIVER.read_text(), "json.decoder.JSONDecodeError")


if __name__ == "__main__":
    unittest.main()
