"""Unit tests for the observation-health gate helper.

Run from repo root:
  python3 -m unittest discover \
    -s docs/experiments/cross-runtime-drift-2026-05/compare \
    -p 'test_*.py'
"""
from __future__ import annotations

import io
import json
import sys
import tarfile
import tempfile
import unittest
from pathlib import Path

THIS_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(THIS_DIR))

import health_gate  # noqa: E402


CLEAN_HEALTH = {
    "schema": "assay.runner.observation_health.v0",
    "run_id": "x",
    "platform": "linux",
    "kernel_layer": "complete",
    "ringbuf_drops": 0,
    "policy_layer": "present",
    "sdk_layer": "present",
    "cgroup_correlation": "clean",
    "notes": [],
}


def _make_archive_dir(tmpdir: Path, health: dict) -> Path:
    (tmpdir / "manifest.json").write_text(
        json.dumps({"schema": "x", "run_id": "y"}), encoding="utf-8"
    )
    (tmpdir / "observation-health.json").write_text(
        json.dumps(health), encoding="utf-8"
    )
    return tmpdir


def _make_archive_tarball(src: Path, dest: Path) -> Path:
    with tarfile.open(dest, "w:gz") as tf:
        for path in sorted(src.rglob("*")):
            if path.is_file():
                tf.add(path, arcname=str(path.relative_to(src)))
    return dest


class EvaluateHealthTests(unittest.TestCase):
    def test_clean_health_returns_empty(self) -> None:
        self.assertEqual(health_gate.evaluate_health(CLEAN_HEALTH), [])

    def test_ringbuf_drops_nonzero_fails(self) -> None:
        h = {**CLEAN_HEALTH, "ringbuf_drops": 42}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("ringbuf_drops", issues[0])

    def test_kernel_layer_partial_fails(self) -> None:
        h = {**CLEAN_HEALTH, "kernel_layer": "partial"}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("kernel_layer", issues[0])

    def test_cgroup_correlation_dirty_fails(self) -> None:
        h = {**CLEAN_HEALTH, "cgroup_correlation": "dirty"}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("cgroup_correlation", issues[0])

    def test_multiple_failures_all_reported(self) -> None:
        h = {
            **CLEAN_HEALTH,
            "ringbuf_drops": 1,
            "kernel_layer": "partial",
            "cgroup_correlation": "dirty",
        }
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 3)

    def test_missing_ringbuf_drops_fails(self) -> None:
        """ringbuf_drops is a required invariant; missing must not be
        silently treated as 0 (P2 review on PR #1348)."""
        h = {k: v for k, v in CLEAN_HEALTH.items() if k != "ringbuf_drops"}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("ringbuf_drops", issues[0])
        self.assertIn("missing", issues[0])

    def test_null_ringbuf_drops_fails(self) -> None:
        h = {**CLEAN_HEALTH, "ringbuf_drops": None}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("ringbuf_drops", issues[0])

    def test_bool_ringbuf_drops_fails(self) -> None:
        # In Python, bool is a subclass of int; True == 1 sneaks past a
        # naive int check. Reject explicitly.
        h = {**CLEAN_HEALTH, "ringbuf_drops": False}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("ringbuf_drops", issues[0])

    def test_missing_kernel_layer_fails(self) -> None:
        h = {k: v for k, v in CLEAN_HEALTH.items() if k != "kernel_layer"}
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("kernel_layer", issues[0])

    def test_missing_cgroup_correlation_fails(self) -> None:
        h = {
            k: v
            for k, v in CLEAN_HEALTH.items()
            if k != "cgroup_correlation"
        }
        issues = health_gate.evaluate_health(h)
        self.assertEqual(len(issues), 1)
        self.assertIn("cgroup_correlation", issues[0])


class HealthGateMainTests(unittest.TestCase):
    def test_clean_directory_archive_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = _make_archive_dir(Path(tmp), CLEAN_HEALTH)
            rc = health_gate.main(["--archive", str(archive)])
            self.assertEqual(rc, 0)

    def test_clean_tarball_archive_passes(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            src.mkdir()
            _make_archive_dir(src, CLEAN_HEALTH)
            tarpath = Path(tmp) / "archive.tar.gz"
            _make_archive_tarball(src, tarpath)
            rc = health_gate.main(["--archive", str(tarpath)])
            self.assertEqual(rc, 0)

    def test_drops_archive_returns_4(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            archive = _make_archive_dir(
                Path(tmp), {**CLEAN_HEALTH, "ringbuf_drops": 5}
            )
            rc = health_gate.main(["--archive", str(archive)])
            self.assertEqual(rc, 4)

    def test_missing_archive_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            rc = health_gate.main(
                ["--archive", str(Path(tmp) / "nope.tar.gz")]
            )
            self.assertEqual(rc, 3)

    def test_missing_health_member_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "manifest.json").write_text(
                "{}", encoding="utf-8"
            )
            rc = health_gate.main(["--archive", str(tmpdir)])
            self.assertEqual(rc, 3)

    def test_corrupt_health_json_returns_3(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            tmpdir = Path(tmp)
            (tmpdir / "observation-health.json").write_text(
                "{not valid", encoding="utf-8"
            )
            rc = health_gate.main(["--archive", str(tmpdir)])
            self.assertEqual(rc, 3)


if __name__ == "__main__":
    unittest.main()
