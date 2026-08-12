#!/usr/bin/env python3
"""Contract + mutation tests for scripts/ci/check_rsa_advisory_removed.py.

The live check against the repository is the durable gate. The fixture mutations
prove the checker fails for the two properties it pins — reintroducing ``rsa``
in resolved metadata, and reintroducing a ``RUSTSEC-2023-0071`` ignore — rather
than for a missing file or a vacuous assertion.
"""

from __future__ import annotations

import importlib.util
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("check_rsa_advisory_removed.py")
SPEC = importlib.util.spec_from_file_location("check_rsa_advisory_removed", MODULE_PATH)
assert SPEC is not None and SPEC.loader is not None
MODULE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MODULE)

REPO_ROOT = Path(
    subprocess.run(
        ["git", "rev-parse", "--show-toplevel"],
        capture_output=True,
        text=True,
        check=True,
    ).stdout.strip()
)


def metadata_without_rsa() -> dict:
    return {
        "packages": [
            {"name": "assay-mcp-server", "version": "5.1.0"},
            {"name": "jsonwebtoken", "version": "11.0.0"},
            {"name": "aws-lc-rs", "version": "1.16.2"},
        ]
    }


def metadata_with_rsa() -> dict:
    meta = metadata_without_rsa()
    meta["packages"].append({"name": "rsa", "version": "0.9.10"})
    return meta


class CheckRsaAdvisoryRemovedTests(unittest.TestCase):
    def test_clean_metadata_and_policy_sites_pass(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in MODULE.POLICY_SITES:
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# clean policy site\n", encoding="utf-8")
            failures = MODULE.evaluate(root, metadata_without_rsa())
            self.assertEqual(failures, [])

    def test_rejects_rsa_package_in_resolved_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in MODULE.POLICY_SITES:
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# clean\n", encoding="utf-8")
            failures = MODULE.evaluate(root, metadata_with_rsa())
            self.assertEqual(len(failures), 1)
            self.assertIn("package named rsa", failures[0])
            self.assertIn("rsa 0.9.10", failures[0])

    def test_rejects_advisory_ignore_in_each_policy_site(self) -> None:
        for site in MODULE.POLICY_SITES:
            with self.subTest(site=site):
                with tempfile.TemporaryDirectory() as tmp:
                    root = Path(tmp)
                    for rel in MODULE.POLICY_SITES:
                        path = root / rel
                        path.parent.mkdir(parents=True, exist_ok=True)
                        body = (
                            f'ignore = ["{MODULE.ADVISORY_ID}"]\n'
                            if rel == site
                            else "# clean\n"
                        )
                        path.write_text(body, encoding="utf-8")
                    failures = MODULE.evaluate(root, metadata_without_rsa())
                    self.assertEqual(len(failures), 1, failures)
                    self.assertIn(MODULE.ADVISORY_ID, failures[0])
                    self.assertIn(site, failures[0])

    def test_mutation_reintroducing_rsa_package_is_caught(self) -> None:
        """Mutation: put package name rsa back into resolved metadata."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in MODULE.POLICY_SITES:
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# clean\n", encoding="utf-8")
            clean = MODULE.evaluate(root, metadata_without_rsa())
            self.assertEqual(clean, [])
            bitten = MODULE.evaluate(root, metadata_with_rsa())
            self.assertTrue(bitten, "reintroducing rsa must fail the check")
            self.assertIn("rsa", bitten[0])

    def test_mutation_reintroducing_ignore_reference_is_caught(self) -> None:
        """Mutation: restore a RUSTSEC-2023-0071 ignore in deny.toml."""
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            for rel in MODULE.POLICY_SITES:
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text("# clean\n", encoding="utf-8")
            deny = root / "deny.toml"
            original = deny.read_text(encoding="utf-8")
            self.assertEqual(MODULE.evaluate(root, metadata_without_rsa()), [])
            deny.write_text(
                original + f'\nignore = ["{MODULE.ADVISORY_ID}"]\n',
                encoding="utf-8",
            )
            bitten = MODULE.evaluate(root, metadata_without_rsa())
            self.assertTrue(
                bitten,
                "reintroducing a RUSTSEC-2023-0071 ignore must fail the check",
            )
            self.assertIn(MODULE.ADVISORY_ID, bitten[0])
            self.assertIn("deny.toml", bitten[0])

    def test_live_repository_has_no_rsa_and_no_advisory_exception(self) -> None:
        """Durable gate: fails on base while rsa/ignore remain; passes after CI-4C."""
        proc = subprocess.run(
            [sys.executable, str(MODULE_PATH)],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            proc.returncode,
            0,
            "live check must pass only when rsa is gone and all four "
            f"{MODULE.ADVISORY_ID} exceptions are removed:\n"
            f"stdout:\n{proc.stdout}\nstderr:\n{proc.stderr}",
        )

    def test_cli_rejects_metadata_file_argv_and_does_not_read_path(self) -> None:
        """CLI must reject path argv; arbitrary paths must not be opened/read."""
        with tempfile.TemporaryDirectory() as tmp:
            probe = Path(tmp) / "must_not_be_read.json"
            # If the CLI still opened this path, evaluate would report rsa from it.
            probe.write_text(json.dumps(metadata_with_rsa()), encoding="utf-8")
            before = probe.read_text(encoding="utf-8")
            proc = subprocess.run(
                [
                    sys.executable,
                    str(MODULE_PATH),
                    "--metadata-file",
                    str(probe),
                ],
                cwd=REPO_ROOT,
                capture_output=True,
                text=True,
                check=False,
            )
            after = probe.read_text(encoding="utf-8")
            self.assertEqual(before, after, "argv path contents must be untouched")
            self.assertNotEqual(proc.returncode, 0, proc.stderr)
            err = proc.stderr
            self.assertRegex(
                err,
                r"(?i)unexpected|unsupported|unknown|reject",
                msg=f"must reject argv path surface, got:\n{err}",
            )
            self.assertNotIn(
                "rsa 0.9.10",
                err,
                "failure must not come from reading the argv path as metadata",
            )

    def test_cli_rejects_unexpected_args(self) -> None:
        proc = subprocess.run(
            [sys.executable, str(MODULE_PATH), "--not-a-real-flag"],
            cwd=REPO_ROOT,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertNotEqual(proc.returncode, 0, proc.stderr)
        self.assertRegex(proc.stderr, r"(?i)unexpected|unsupported|unknown|reject")

    def test_cli_accepts_metadata_on_stdin(self) -> None:
        """Stdin metadata injection remains the supported non-live path."""
        # Clean metadata + live clean policy sites → PASS.
        proc = subprocess.run(
            [sys.executable, str(MODULE_PATH)],
            cwd=REPO_ROOT,
            input=json.dumps(metadata_without_rsa()),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(
            proc.returncode,
            0,
            f"stdin clean metadata must pass:\n{proc.stdout}\n{proc.stderr}",
        )
        # rsa on stdin must still fail closed on the package property.
        bad = subprocess.run(
            [sys.executable, str(MODULE_PATH)],
            cwd=REPO_ROOT,
            input=json.dumps(metadata_with_rsa()),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(bad.returncode, 1, bad.stderr)
        self.assertIn("package named rsa", bad.stderr)


if __name__ == "__main__":
    unittest.main()
