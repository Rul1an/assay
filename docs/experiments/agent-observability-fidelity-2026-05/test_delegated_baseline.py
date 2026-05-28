"""Tests for the delegated semantic-gap baseline smoke record."""

from __future__ import annotations

import json
import unittest
from pathlib import Path
from typing import Any

from test_evidence_pack import assert_matches_schema

ROOT = Path(__file__).resolve().parent
RUN_ROOT = ROOT / "runs" / "slice7-delegated-baseline"
OBSERVABILITY_SCHEMA_ROOT = (
    ROOT.parent.parent / "reference" / "observability" / "schema"
)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_schema(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


class DelegatedBaselineSmokeTests(unittest.TestCase):
    def test_proof_pack_reference_pins_successful_delegated_run(self) -> None:
        reference = load_json(RUN_ROOT / "proof-pack-reference.json")

        self.assertEqual(reference["scenario_id"], "matched_safe_read")
        self.assertEqual(reference["status"], "delegated-baseline-smoke-verified")
        self.assertEqual(reference["workflow"]["run_id"], "26570812096")
        self.assertEqual(
            reference["workflow"]["run_url"],
            "https://github.com/Rul1an/assay/actions/runs/26570812096",
        )
        self.assertEqual(
            reference["workflow"]["head_sha"],
            "32952e2cbc79b41a5951cff5408a82528bde8ad9",
        )
        self.assertEqual(
            reference["workflow"]["inputs"],
            {"gates": "openai-agents-kernel-policy", "build_ebpf": "true"},
        )
        self.assertEqual(reference["proof_pack"]["gate_status"], "passed")
        self.assertEqual(reference["proof_pack"]["archive_count"], 3)
        self.assertEqual(len(reference["proof_pack"]["pass_lines"]), 4)

    def test_delegated_health_and_join_invariants_are_clean(self) -> None:
        reference = load_json(RUN_ROOT / "proof-pack-reference.json")
        health = reference["health"]
        evidence = reference["runner_archive_evidence"]

        self.assertEqual(health["kernel_layer"], "complete")
        self.assertEqual(health["ringbuf_drops"], 0)
        self.assertEqual(health["cgroup_correlation"], "clean")
        self.assertEqual(health["correlation_status"], "clean")
        self.assertEqual(health["ambiguities"], 0)
        self.assertEqual(evidence["tool_call_id"], "tc_runner_policy_001")
        self.assertEqual(evidence["tool"], "read_file")
        self.assertEqual(evidence["policy_decision"], "allow")
        self.assertEqual(evidence["kernel_event_count"], 2)
        for path in evidence["measured_paths"]:
            self.assertTrue(path.startswith(evidence["workdir_prefix"]), path)
        self.assertTrue(
            any(path.endswith("/openai-agents-input.txt") for path in evidence["measured_paths"])
        )

    def test_review_rows_match_reference_schemas(self) -> None:
        join_schema = load_schema(
            OBSERVABILITY_SCHEMA_ROOT / "join-result-v0.schema.json"
        )
        claim_schema = load_schema(
            OBSERVABILITY_SCHEMA_ROOT / "claim-class-cell-v0.schema.json"
        )
        verdict_schema = load_schema(
            ROOT / "schema" / "semantic-gap-verdict-v0.schema.json"
        )
        redaction_schema = load_schema(
            ROOT / "schema" / "redaction-manifest-v0.schema.json"
        )

        join = load_json(RUN_ROOT / "join-result.json")
        verdict = load_json(RUN_ROOT / "scenario-verdict.json")
        redaction = load_json(RUN_ROOT / "redaction-manifest.json")
        cells = load_json(RUN_ROOT / "claim-class-cells.json")

        assert_matches_schema(self, join, join_schema, root=join_schema)
        assert_matches_schema(self, verdict, verdict_schema, root=verdict_schema)
        assert_matches_schema(
            self, redaction, redaction_schema, root=redaction_schema
        )
        for index, cell in enumerate(cells):
            assert_matches_schema(
                self,
                cell,
                claim_schema,
                root=claim_schema,
                path=f"$[{index}]",
            )

    def test_scenario_verdict_is_positive_join_only(self) -> None:
        verdict = load_json(RUN_ROOT / "scenario-verdict.json")
        join = load_json(RUN_ROOT / "join-result.json")
        cells = load_json(RUN_ROOT / "claim-class-cells.json")

        self.assertEqual(verdict["verdict"], "positive_join")
        self.assertEqual(verdict["evidence_pack_claim_class"], "positive_join")
        self.assertEqual(verdict["runner_health_status"], "clean")
        self.assertEqual(verdict["trace_calibration_status"], "clean")
        self.assertEqual(verdict["join_key"], "tool_call_id")
        self.assertEqual(verdict["join_grade"], "strong")
        self.assertFalse(verdict["fallback_used"])
        self.assertEqual(join["join_value"], "tc_runner_policy_001")
        self.assertEqual(join["join_grade"], "strong")
        self.assertFalse(join["fallback_used"])
        self.assertEqual(
            [cell["claim_type"] for cell in cells],
            [
                "reported_tool_intent",
                "measured_filesystem_effect",
                "joined_positive_baseline",
            ],
        )
        self.assertTrue(
            all(
                "does_not_publish_delegated_gap_finding" in cell["non_claims"]
                for cell in cells
            )
        )


if __name__ == "__main__":
    unittest.main()
