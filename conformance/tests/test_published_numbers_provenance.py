#!/usr/bin/env python3
"""Fail-closed classification of published-number measurement commits.

    python3 conformance/tests/test_published_numbers_provenance.py

The hole is check_published_numbers.py:248-259 on edc9df0c085ad51724ed29d9eee7d905568b49b9:
`git diff <measured-at> HEAD` then `continue` on nonzero, so an unavailable commit
produced no finding. These controls fire each classification on purpose.
"""

from __future__ import annotations

import contextlib
import io
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/adequacy"))

import check_published_numbers as chk  # noqa: E402

# Reuse the existing sandbox so file reads stay off the working tree.
from test_published_numbers_guard import sandbox  # noqa: E402


class ClassifyMeasuredCommit(unittest.TestCase):
    """One function, four outcomes. Hostile strings never become git revisions."""

    HOSTILE = (
        "'; rm -rf /'",
        "-n",
        "HEAD",
        "origin/main",
        "EDC9DF0C085AD51724ED29D9EEE7D905568B49B9",
        "a" * 39,
        "a" * 41,
        "\u00e9" + "a" * 39,
        "",
        "../" + "a" * 37,
        "deadbeef\n" + "a" * 31,
    )
    UNREACHABLE = "deadbeef" * 5

    def _git(self, repo: Path, *args: str) -> subprocess.CompletedProcess:
        env = {
            **os.environ,
            "GIT_CONFIG_GLOBAL": "/dev/null",
            "GIT_CONFIG_SYSTEM": "/dev/null",
            "GIT_AUTHOR_NAME": "t",
            "GIT_AUTHOR_EMAIL": "t@t.example",
            "GIT_COMMITTER_NAME": "t",
            "GIT_COMMITTER_EMAIL": "t@t.example",
        }
        return subprocess.run(
            ["git", "-C", str(repo), *args],
            capture_output=True,
            text=True,
            timeout=30,
            check=True,
            env=env,
        )

    def _tiny_repo(self, root: Path, files: dict, later: dict | None = None) -> str:
        self._git(root, "init", "-b", "main")
        for rel, body in files.items():
            path = root / rel
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text(body, encoding="utf-8")
            self._git(root, "add", "--", rel)
        self._git(
            root,
            "-c",
            "user.email=t@t.example",
            "-c",
            "user.name=t",
            "commit",
            "-m",
            "one",
        )
        first = self._git(root, "rev-parse", "HEAD").stdout.strip()
        self.assertRegex(first, r"^[0-9a-f]{40}$")
        if later:
            for rel, body in later.items():
                path = root / rel
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(body, encoding="utf-8")
                self._git(root, "add", "--", rel)
            self._git(
                root,
                "-c",
                "user.email=t@t.example",
                "-c",
                "user.name=t",
                "commit",
                "-m",
                "two",
            )
        return first

    def test_malformed_commits_are_not_offered_to_git(self):
        """CONTROL: hostile / wrong-shape strings are malformed, never resolved."""
        with tempfile.TemporaryDirectory() as raw:
            repo = Path(raw)
            self._tiny_repo(repo, {"keep.txt": "a\n"})
            for commit in self.HOSTILE:
                with self.subTest(commit=commit):
                    kind, extra = chk.classify_measured_commit(commit, ["keep.txt"], repo)
                    self.assertEqual(kind, chk.MALFORMED)
                    self.assertEqual(extra, [])

    def test_a_valid_looking_but_unreachable_commit_is_not_clean(self):
        """CONTROL: 40-hex that git cannot resolve is unreachable, not no-drift."""
        with tempfile.TemporaryDirectory() as raw:
            repo = Path(raw)
            self._tiny_repo(repo, {"keep.txt": "a\n"})
            kind, extra = chk.classify_measured_commit(self.UNREACHABLE, ["keep.txt"], repo)
            self.assertEqual(kind, chk.UNREACHABLE)
            self.assertEqual(extra, [])

    def test_a_dirty_generated_result_is_dirty(self):
        """CONTROL: depends_on that moved since the measurement is dirty."""
        with tempfile.TemporaryDirectory() as raw:
            repo = Path(raw)
            first = self._tiny_repo(repo, {"keep.txt": "a\n"}, later={"keep.txt": "b\n"})
            kind, extra = chk.classify_measured_commit(first, ["keep.txt"], repo)
            self.assertEqual(kind, chk.DIRTY)
            self.assertIn("keep.txt", extra)

    def test_true_no_drift_is_clean(self):
        """CONTROL: a reachable commit whose depends_on did not move is clean."""
        with tempfile.TemporaryDirectory() as raw:
            repo = Path(raw)
            first = self._tiny_repo(repo, {"keep.txt": "a\n"})
            kind, extra = chk.classify_measured_commit(first, ["keep.txt"], repo)
            self.assertEqual(kind, chk.CLEAN)
            self.assertEqual(extra, [])


class CheckerFailsClosedOnUnresolved(unittest.TestCase):
    """The caller's findings and exit must not stay clean when resolution fails.

    Restoring `if moved.returncode != 0: continue` makes
    test_an_unreachable_measured_commit_makes_the_checker_red stay green.
    """

    UNREACHABLE = "deadbeef" * 5

    def assert_red(self, needle: str):
        findings = chk.check()
        self.assertTrue(findings, "the checker stayed green; this guard is wired to nothing")
        self.assertTrue(
            any(needle in f for f in findings),
            "went red for the wrong reason: %s" % findings,
        )

    def test_an_unreachable_measured_commit_makes_the_checker_red(self):
        """CONTROL: results.json names a 40-hex commit this checkout does not have."""
        with sandbox() as root:
            self.assertEqual(chk.check(), [])
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            doc["corpora"][0]["measured_at"]["commit"] = self.UNREACHABLE
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            self.assert_red("cannot be resolved")
            self.assertEqual(chk.main(["--json"]), 1)
            # restore so the next assertion is about this control, not leftover state
            res.write_text(
                (REPO / "conformance/adequacy/results.json").read_text(encoding="utf-8"),
                encoding="utf-8",
            )
            self.assertEqual(chk.check(), [])

    def test_a_malformed_measured_commit_makes_the_checker_red(self):
        """CONTROL: origin/main is malformed, not a git revision."""
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            doc["corpora"][0]["measured_at"]["commit"] = "origin/main"
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            self.assert_red("malformed")
            self.assertEqual(chk.main(["--json"]), 1)

    def test_required_context_is_not_clean_when_resolution_is_skipped(self):
        """JSON ok must be false when a commit is unresolved."""
        with sandbox() as root:
            res = root / "conformance/adequacy/results.json"
            doc = json.loads(res.read_text())
            doc["corpora"][0]["measured_at"]["commit"] = self.UNREACHABLE
            res.write_text(json.dumps(doc, indent=2, sort_keys=True) + "\n")
            buf = io.StringIO()
            with contextlib.redirect_stdout(buf):
                code = chk.main(["--json"])
            payload = json.loads(buf.getvalue())
            self.assertEqual(code, 1)
            self.assertFalse(payload["ok"])
            self.assertTrue(payload["findings"])


    def test_checker_does_no_network(self):
        src = Path(chk.__file__).read_text(encoding="utf-8")
        self.assertNotIn("git fetch", src)
        self.assertNotIn("shell=True", src)


if __name__ == "__main__":
    unittest.main(verbosity=2)
