"""Tests for the semantic-gap synthetic harness."""

from __future__ import annotations

import importlib.util
import io
import json
import sys
import tempfile
import unittest
from contextlib import redirect_stdout
from pathlib import Path
from typing import Any

from test_evidence_pack import assert_matches_schema

ROOT = Path(__file__).resolve().parent
HARNESS_PATH = ROOT / "semantic_gap_harness.py"
OBSERVABILITY_SCHEMA_ROOT = (
    ROOT.parent.parent / "reference" / "observability" / "schema"
)

spec = importlib.util.spec_from_file_location("semantic_gap_harness", HARNESS_PATH)
assert spec is not None and spec.loader is not None
semantic_gap_harness = importlib.util.module_from_spec(spec)
sys.modules["semantic_gap_harness"] = semantic_gap_harness
spec.loader.exec_module(semantic_gap_harness)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_schema(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


class SemanticGapHarnessTests(unittest.TestCase):
    def generate(self, root: Path) -> Path:
        out = root / "semantic-gap-runs"
        semantic_gap_harness.generate_harness(
            out_dir=out,
            scenarios=list(semantic_gap_harness.SYNTHETIC_SCENARIOS),
            created_at="2026-05-28T08:00:00Z",
            redaction_policy="none",
        )
        return out

    def test_synthetic_harness_emits_all_scenarios_with_evidence_packs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))

            self.assertEqual(
                sorted(path.name for path in out.iterdir()),
                [
                    "hidden_write",
                    "matched_safe_read",
                    "path_rewrite",
                    "retry_self_correction",
                    "runtime_side_effect",
                    "weak_join_fallback",
                ],
            )
            for scenario_id in semantic_gap_harness.SYNTHETIC_SCENARIOS:
                scenario_dir = out / scenario_id
                for name in (
                    "trace.json",
                    "runner-archive.json",
                    "observation-health.json",
                    "join-result.json",
                    "claim-class-cells.json",
                    "scenario-verdict.json",
                    "summary.md",
                    "evidence-pack/manifest.json",
                    "evidence-pack/summary.md",
                    "evidence-pack/redaction-manifest.json",
                ):
                    self.assertTrue((scenario_dir / name).exists(), name)
                manifest = load_json(scenario_dir / "evidence-pack/manifest.json")
                self.assertEqual(manifest["scenario_id"], scenario_id)
                self.assertEqual(manifest["observation_health_status"], "clean")

    def test_matched_safe_read_is_positive_strong_join(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "matched_safe_read"
            join = load_json(out / "join-result.json")
            verdict = load_json(out / "scenario-verdict.json")
            manifest = load_json(out / "evidence-pack/manifest.json")

            self.assertEqual(join["schema"], "assay.observability.join_result.v0")
            self.assertEqual(join["join_key"], "tool_call_id")
            self.assertEqual(join["join_grade"], "strong")
            self.assertFalse(join["fallback_used"])
            self.assertEqual(verdict["verdict"], "positive_join")
            self.assertEqual(manifest["claim_class"], "positive_join")

    def test_path_rewrite_records_symlink_projection_gap(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "path_rewrite"
            trace = load_json(out / "trace.json")
            archive = load_json(out / "runner-archive.json")
            verdict = load_json(out / "scenario-verdict.json")
            cells = load_json(out / "claim-class-cells.json")

            self.assertEqual(
                trace["tool_calls"][0]["reported_path"], "safe-link.txt"
            )
            self.assertEqual(archive["effects"][0]["path"], "safe.txt")
            self.assertEqual(
                archive["effects"][0]["resolved_from"], "safe-link.txt"
            )
            self.assertEqual(verdict["verdict"], "semantic_gap")
            self.assertIn("does_not_claim_unsafe_behavior", cells[2]["non_claims"])

    def test_hidden_write_is_semantic_gap_with_same_tool_call_join(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "hidden_write"
            trace = load_json(out / "trace.json")
            archive = load_json(out / "runner-archive.json")
            join = load_json(out / "join-result.json")
            verdict = load_json(out / "scenario-verdict.json")
            cells = load_json(out / "claim-class-cells.json")

            self.assertEqual(
                trace["schema"],
                semantic_gap_harness.SYNTHETIC_TRACE_SCHEMA,
            )
            self.assertEqual(
                archive["schema"],
                semantic_gap_harness.SYNTHETIC_RUNNER_ARCHIVE_SCHEMA,
            )
            self.assertEqual(
                trace["tool_calls"][0]["tool_call_id"],
                archive["effects"][0]["tool_call_id"],
            )
            self.assertEqual(trace["tool_calls"][0]["reported_action"], "read")
            self.assertEqual(archive["effects"][0]["effect"], "create_write")
            self.assertEqual(join["join_grade"], "strong")
            self.assertEqual(verdict["verdict"], "semantic_gap")
            self.assertEqual(cells[2]["artifact_role"], "joined_artifacts")
            self.assertIn(
                "does_not_claim_malicious_behavior", cells[2]["non_claims"]
            )

    def test_retry_self_correction_preserves_prior_attempts(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "retry_self_correction"
            trace = load_json(out / "trace.json")
            archive = load_json(out / "runner-archive.json")
            join = load_json(out / "join-result.json")
            verdict = load_json(out / "scenario-verdict.json")

            self.assertEqual(trace["tool_calls"][0]["reported_status"], "success")
            self.assertEqual(len(archive["effects"]), 3)
            self.assertEqual(
                [effect["effect"] for effect in archive["effects"]],
                ["failed_open", "failed_open", "read"],
            )
            self.assertEqual(
                trace["tool_calls"][0]["tool_call_id"],
                archive["effects"][0]["tool_call_id"],
            )
            self.assertEqual(join["join_grade"], "strong")
            self.assertEqual(verdict["verdict"], "semantic_gap")

    def test_runtime_side_effect_is_run_scope_diagnostic_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "runtime_side_effect"
            trace = load_json(out / "trace.json")
            archive = load_json(out / "runner-archive.json")
            join = load_json(out / "join-result.json")
            verdict = load_json(out / "scenario-verdict.json")
            cells = load_json(out / "claim-class-cells.json")
            manifest = load_json(out / "evidence-pack/manifest.json")

            self.assertEqual(trace["tool_calls"], [])
            self.assertTrue(
                archive["effects"][0]["emitted_before_first_tool_call"]
            )
            self.assertEqual(join["join_key"], "run_id")
            self.assertEqual(join["scope"], "run")
            self.assertEqual(join["join_grade"], "diagnostic")
            self.assertIn("trace.json#/tool_calls", join["evidence_refs"])
            self.assertEqual(verdict["verdict"], "diagnostic_only")
            self.assertEqual(manifest["claim_class"], "diagnostic")
            self.assertEqual(cells[0]["artifact_role"], "none")
            self.assertEqual(cells[0]["claim_strength"], "absent")
            self.assertEqual(cells[0]["evidence_refs"], [])

    def test_weak_join_fallback_stays_diagnostic_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp)) / "weak_join_fallback"
            join = load_json(out / "join-result.json")
            verdict = load_json(out / "scenario-verdict.json")
            manifest = load_json(out / "evidence-pack/manifest.json")

            self.assertEqual(join["join_key"], "timestamp_or_order")
            self.assertEqual(join["join_grade"], "diagnostic")
            self.assertTrue(join["fallback_used"])
            self.assertIn("ambiguous_proximity", join["notes"])
            self.assertEqual(verdict["verdict"], "diagnostic_only")
            self.assertEqual(manifest["claim_class"], "diagnostic")

    def test_outputs_match_pinned_schemas(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            join_schema = load_schema(
                OBSERVABILITY_SCHEMA_ROOT / "join-result-v0.schema.json"
            )
            claim_schema = load_schema(
                OBSERVABILITY_SCHEMA_ROOT / "claim-class-cell-v0.schema.json"
            )
            verdict_schema = load_schema(
                ROOT / "schema" / "semantic-gap-verdict-v0.schema.json"
            )

            for scenario_id in semantic_gap_harness.SYNTHETIC_SCENARIOS:
                scenario_dir = out / scenario_id
                join = load_json(scenario_dir / "join-result.json")
                verdict = load_json(scenario_dir / "scenario-verdict.json")
                cells = load_json(scenario_dir / "claim-class-cells.json")
                assert_matches_schema(self, join, join_schema, root=join_schema)
                assert_matches_schema(
                    self, verdict, verdict_schema, root=verdict_schema
                )
                for index, cell in enumerate(cells):
                    assert_matches_schema(
                        self,
                        cell,
                        claim_schema,
                        root=claim_schema,
                        path=f"$[{index}]",
                    )

    def test_verdict_schema_constrains_inconclusive_to_diagnostic(self) -> None:
        schema = load_schema(ROOT / "schema" / "semantic-gap-verdict-v0.schema.json")
        inconclusive_rules = [
            rule
            for rule in schema["allOf"]
            if rule["if"]["properties"]["verdict"].get("const") == "inconclusive"
        ]

        self.assertEqual(len(inconclusive_rules), 1)
        then_properties = inconclusive_rules[0]["then"]["properties"]
        self.assertEqual(
            then_properties["evidence_pack_claim_class"]["const"], "diagnostic"
        )
        self.assertNotIn("trace_calibration_status", then_properties)

    def test_inconclusive_verdict_validates_only_as_diagnostic(self) -> None:
        schema = load_schema(ROOT / "schema" / "semantic-gap-verdict-v0.schema.json")
        payload = {
            "schema": semantic_gap_harness.SEMANTIC_GAP_VERDICT_SCHEMA,
            "scenario_id": "matched_safe_read",
            "role": "baseline",
            "verdict": "inconclusive",
            "evidence_pack_claim_class": "diagnostic",
            "runner_health_status": "inconclusive",
            "trace_calibration_status": "lossy",
            "join_key": "tool_call_id",
            "join_grade": "failed",
            "fallback_used": False,
            "reason": "Lossy calibration prevents a semantic claim.",
            "non_claims": ["does_not_publish_delegated_gap_finding"],
        }

        assert_matches_schema(self, payload, schema, root=schema)
        payload["evidence_pack_claim_class"] = "positive_join"
        with self.assertRaises(AssertionError):
            assert_matches_schema(self, payload, schema, root=schema)

    def test_synthetic_scenarios_match_verdict_schema_enum(self) -> None:
        schema = load_schema(ROOT / "schema" / "semantic-gap-verdict-v0.schema.json")

        self.assertEqual(
            sorted(schema["properties"]["scenario_id"]["enum"]),
            sorted(semantic_gap_harness.SYNTHETIC_SCENARIOS),
        )
        self.assertEqual(
            sorted(semantic_gap_harness.scenario_definitions()),
            sorted(semantic_gap_harness.SYNTHETIC_SCENARIOS),
        )
        self.assertLessEqual(
            set(semantic_gap_harness.MVP_SCENARIOS),
            set(semantic_gap_harness.SYNTHETIC_SCENARIOS),
        )

    def test_cli_generates_selected_scenario(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "semantic-gap-runs"
            stdout = io.StringIO()

            with redirect_stdout(stdout):
                exit_code = semantic_gap_harness.main(
                    [
                        "--out-dir",
                        str(out),
                        "--scenario",
                        "matched_safe_read",
                        "--created-at",
                        "2026-05-28T08:00:00Z",
                    ]
                )

            self.assertEqual(exit_code, 0)
            self.assertIn("matched_safe_read", stdout.getvalue())
            self.assertTrue(
                (out / "matched_safe_read" / "scenario-verdict.json").exists()
            )
            self.assertFalse((out / "hidden_write").exists())

    def test_existing_nonempty_output_directory_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "semantic-gap-runs"
            out.mkdir()
            (out / "stale.txt").write_text("stale\n", encoding="utf-8")

            with self.assertRaises(FileExistsError):
                semantic_gap_harness.generate_harness(
                    out_dir=out,
                    scenarios=["matched_safe_read"],
                    created_at="2026-05-28T08:00:00Z",
                    redaction_policy="none",
                )


if __name__ == "__main__":
    unittest.main()
