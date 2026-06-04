import importlib.util
import json
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("annotate_drift.py")
FIXTURES = Path(__file__).with_name("fixtures")
REQUIRED_CELL_FIELDS = {
    "schema",
    "claim_type",
    "artifact_role",
    "claim_strength",
    "claim_basis",
    "evidence_refs",
    "notes",
    "non_claims",
}


def _load_module():
    spec = importlib.util.spec_from_file_location("annotate_drift", MODULE_PATH)
    assert spec is not None and spec.loader is not None
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _report(name: str) -> dict:
    return json.loads((FIXTURES / name).read_text(encoding="utf-8"))


class CoverageAwareDriftTest(unittest.TestCase):
    def setUp(self) -> None:
        self.m = _load_module()
        self.annotation = self.m.annotate(_report("drift_report.json"))

    def _cells(self) -> dict:
        return {c["claim_type"]: c for c in self.annotation["claim_cells"]}

    def test_positive_drift_is_partial_measured_not_strong(self):
        cells = self._cells()
        fs = cells["measured_filesystem_paths_touched_drift"]
        self.assertEqual(fs["claim_strength"], "partial")
        self.assertEqual(fs["claim_basis"], "measured")
        # the report does not surface health, so positive is never strong here
        self.assertFalse(
            any(
                c["claim_strength"] == "strong"
                for c in self.annotation["claim_cells"]
            )
        )

    def test_exhaustive_equality_is_weak_with_blind_spot_note(self):
        cells = self._cells()
        net = cells["exhaustive_network_endpoints_equality"]
        self.assertEqual(net["claim_strength"], "weak")
        self.assertTrue(any("QUIC" in note for note in net["notes"]))

    def test_bounded_negative_is_blocked_for_measured_dimensions(self):
        blocked = {b["requested_claim"] for b in self.annotation["blocked_claims"]}
        self.assertIn("no_filesystem_paths_touched_effect_beyond_observed", blocked)
        self.assertIn("no_network_endpoints_effect_beyond_observed", blocked)
        # even the empty/inconclusive process row blocks the absence claim
        self.assertIn("no_process_execs_effect_beyond_observed", blocked)

    def test_empty_dimension_emits_no_positive_or_exhaustive_cell(self):
        cells = self._cells()
        self.assertNotIn("measured_process_execs_drift", cells)
        self.assertNotIn("exhaustive_process_execs_equality", cells)

    def test_task_induced_is_caveated_not_treated_as_equality(self):
        caveats = {c["dimension"]: c for c in self.annotation["classification_caveats"]}
        self.assertIn("network_endpoints", caveats)
        self.assertIn("not proof", caveats["network_endpoints"]["caveat"])

    def test_reported_dimension_is_reported_basis_no_coverage_gate(self):
        cells = self._cells()
        sdk = cells["reported_sdk_tool_events"]
        self.assertEqual(sdk["claim_basis"], "reported")
        # no exhaustive/blocked claim is derived for a reported dimension
        self.assertNotIn("exhaustive_sdk_tool_events_equality", cells)
        self.assertFalse(
            any("sdk_tool_events" in b["requested_claim"] for b in self.annotation["blocked_claims"])
        )

    def test_all_cells_conform_to_required_fields(self):
        for cell in self.annotation["claim_cells"]:
            self.assertEqual(REQUIRED_CELL_FIELDS - set(cell), set(), cell["claim_type"])

    def test_wrong_source_schema_is_rejected(self):
        with self.assertRaises(ValueError):
            self.m.annotate({"schema": "assay.runner.runtime_drift.v0", "rows": []})

    def test_matches_frozen_expected(self):
        expected = _report("expected_annotation.json")
        self.assertEqual(self.annotation, expected)


if __name__ == "__main__":
    unittest.main()
