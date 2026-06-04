import importlib.util
import json
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("report_from_archive.py")
FIXTURES = Path(__file__).with_name("fixtures")


def _load_module():
    spec = importlib.util.spec_from_file_location("coverage_aware_report", MODULE_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _archive(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


class CoverageAwareReportTest(unittest.TestCase):
    def setUp(self) -> None:
        self.m = _load_module()

    def test_clean_positive_is_strong_measured(self):
        report = self.m.build_report(_archive("clean.archive.json"))
        positives = {
            c["claim_type"]: c
            for c in report["claim_cells"]
            if c["claim_type"].startswith("measured_")
        }
        self.assertEqual(positives["measured_filesystem_effect"]["claim_strength"], "strong")
        self.assertEqual(positives["measured_filesystem_effect"]["claim_basis"], "measured")
        self.assertEqual(positives["measured_network_effect"]["claim_strength"], "strong")

    def test_clean_exhaustive_network_is_weak(self):
        report = self.m.build_report(_archive("clean.archive.json"))
        exhaustive = {
            c["claim_type"]: c
            for c in report["claim_cells"]
            if c["claim_type"].startswith("exhaustive_")
        }
        self.assertEqual(exhaustive["exhaustive_network_set"]["claim_strength"], "weak")
        self.assertTrue(
            any("QUIC" in note for note in exhaustive["exhaustive_network_set"]["notes"])
        )

    def test_clean_bounded_negative_is_blocked(self):
        report = self.m.build_report(_archive("clean.archive.json"))
        blocked = {b["requested_claim"] for b in report["blocked_claims"]}
        self.assertIn("no_unexpected_network_effect", blocked)
        self.assertIn("no_unexpected_filesystem_effect", blocked)
        # no bounded-negative claim is emitted as an allowed/strong cell
        self.assertFalse(
            any(c["claim_type"] == "bounded_negative_claim" for c in report["claim_cells"])
        )

    def test_clipped_capture_downgrades_positive_to_partial(self):
        report = self.m.build_report(_archive("clipped.archive.json"))
        positives = {
            c["claim_type"]: c
            for c in report["claim_cells"]
            if c["claim_type"].startswith("measured_")
        }
        # ringbuf drops > 0 -> capture not clean -> positive is partial, not strong
        self.assertEqual(positives["measured_filesystem_effect"]["claim_strength"], "partial")

    def test_unobserved_dimension_is_absent_not_claimed(self):
        report = self.m.build_report(_archive("clean.archive.json"))
        # process_execs is empty -> no process claim cell at all
        self.assertFalse(
            any("process" in c["claim_type"] for c in report["claim_cells"])
        )

    def test_missing_observation_health_is_rejected(self):
        with self.assertRaises(ValueError):
            self.m.build_report({"capability_surface": {"filesystem_paths": []}})

    def test_not_applicable_makes_positive_absent(self):
        # A valid not_applicable record (non-Linux, kernel_layer=absent) has no
        # measured kernel surface: the positive cell is absent with empty
        # evidence_refs, not an overstated partial.
        archive = {
            "observation_health": {
                "schema": "assay.runner.observation_health.v0",
                "run_id": "run_blocked",
                "platform": "darwin",
                "kernel_layer": "absent",
                "ringbuf_drops": 0,
                "cgroup_correlation": "clean",
            },
            "capability_surface": {
                "schema": "assay.runner.capability_surface.v0",
                "run_id": "run_blocked",
                "filesystem_paths": ["/etc/hosts"],
            },
        }
        report = self.m.build_report(archive)
        fs = next(
            c for c in report["claim_cells"]
            if c["claim_type"] == "measured_filesystem_effect"
        )
        self.assertEqual(fs["claim_strength"], "absent")
        self.assertEqual(fs["artifact_role"], "none")
        self.assertEqual(fs["evidence_refs"], [])

    def test_failed_cgroup_correlation_is_rejected(self):
        # cgroup_correlation=failed is a non-passing observation_health record;
        # the sample rejects it rather than interpreting it.
        archive = {
            "observation_health": {
                "schema": "assay.runner.observation_health.v0",
                "run_id": "run_failed",
                "platform": "linux",
                "kernel_layer": "complete",
                "ringbuf_drops": 0,
                "cgroup_correlation": "failed",
            },
            "capability_surface": {
                "schema": "assay.runner.capability_surface.v0",
                "run_id": "run_failed",
                "filesystem_paths": ["/etc/hosts"],
            },
        }
        with self.assertRaises(ValueError):
            self.m.build_report(archive)

    def test_mismatched_run_id_is_rejected(self):
        archive = {
            "observation_health": {
                "schema": "assay.runner.observation_health.v0",
                "run_id": "run_a",
                "platform": "linux",
                "kernel_layer": "complete",
                "ringbuf_drops": 0,
                "cgroup_correlation": "clean",
            },
            "capability_surface": {
                "schema": "assay.runner.capability_surface.v0",
                "run_id": "run_b",
                "filesystem_paths": ["/etc/hosts"],
            },
        }
        with self.assertRaises(ValueError):
            self.m.build_report(archive)

    def test_malformed_surface_field_is_rejected(self):
        # A non-list (or empty-string-containing) capability_surface field is
        # refused rather than interpreted as observed effects.
        archive = {
            "observation_health": {
                "schema": "assay.runner.observation_health.v0",
                "run_id": "run_bad_surface",
                "platform": "linux",
                "kernel_layer": "complete",
                "ringbuf_drops": 0,
                "cgroup_correlation": "clean",
            },
            "capability_surface": {
                "schema": "assay.runner.capability_surface.v0",
                "run_id": "run_bad_surface",
                "filesystem_paths": "/etc/hosts",
            },
        }
        with self.assertRaises(ValueError):
            self.m.build_report(archive)

    def test_clean_report_matches_frozen_fixture(self):
        # Golden test: the generator output must equal the frozen expected
        # report, so the fixture and generator cannot drift apart silently.
        report = self.m.build_report(_archive("clean.archive.json"))
        expected = json.loads((FIXTURES / "clean.report.json").read_text(encoding="utf-8"))
        self.assertEqual(report, expected)


if __name__ == "__main__":
    unittest.main()
