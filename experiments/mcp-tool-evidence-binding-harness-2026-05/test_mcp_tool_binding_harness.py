"""Tests for the synthetic MCP tool evidence-binding harness."""

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

ROOT = Path(__file__).resolve().parent
HARNESS_PATH = ROOT / "mcp_tool_binding_harness.py"
FIDELITY_ROOT = ROOT.parent / "agent-observability-fidelity-2026-05"
SCHEMA_PATH = ROOT / "schema" / "mcp-tool-binding-cell-v0.schema.json"
STABLE_RUNS = ROOT / "runs" / "starter-synthetic"

schema_spec = importlib.util.spec_from_file_location(
    "fidelity_test_evidence_pack",
    FIDELITY_ROOT / "test_evidence_pack.py",
)
assert schema_spec is not None and schema_spec.loader is not None
schema_helpers = importlib.util.module_from_spec(schema_spec)
sys.modules["fidelity_test_evidence_pack"] = schema_helpers
schema_spec.loader.exec_module(schema_helpers)
assert_matches_schema = schema_helpers.assert_matches_schema

harness_spec = importlib.util.spec_from_file_location(
    "mcp_tool_binding_harness",
    HARNESS_PATH,
)
assert harness_spec is not None and harness_spec.loader is not None
mcp_tool_binding_harness = importlib.util.module_from_spec(harness_spec)
sys.modules["mcp_tool_binding_harness"] = mcp_tool_binding_harness
harness_spec.loader.exec_module(mcp_tool_binding_harness)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


class McpToolBindingHarnessTests(unittest.TestCase):
    def generate(self, root: Path) -> Path:
        out = root / "mcp-tool-binding-runs"
        mcp_tool_binding_harness.generate_harness(
            out_dir=out,
            scenarios=list(mcp_tool_binding_harness.STARTER_SCENARIOS),
            assay_commit="test-assay-commit",
            created_at="2026-05-29T00:00:00Z",
        )
        return out

    def cell_for(self, out: Path, scenario_id: str) -> dict[str, Any]:
        return load_json(out / scenario_id / "binding-cell.json")

    def test_harness_emits_all_starter_scenarios(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))

            self.assertEqual(
                sorted(path.name for path in out.iterdir()),
                sorted(mcp_tool_binding_harness.STARTER_SCENARIOS),
            )
            for scenario_id in mcp_tool_binding_harness.STARTER_SCENARIOS:
                scenario_dir = out / scenario_id
                self.assertTrue((scenario_dir / "binding-cell.json").exists())
                self.assertTrue((scenario_dir / "context-descriptor-set.json").exists())
                self.assertTrue((scenario_dir / "summary.md").exists())

    def test_outputs_match_pinned_schema(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            schema = load_json(SCHEMA_PATH)

            for scenario_id in mcp_tool_binding_harness.STARTER_SCENARIOS:
                assert_matches_schema(
                    self,
                    self.cell_for(out, scenario_id),
                    schema,
                    root=schema,
                    path=f"$[{scenario_id}]",
                )

    def test_schema_enum_matches_harness_constants(self) -> None:
        schema = load_json(SCHEMA_PATH)

        self.assertEqual(
            sorted(schema["properties"]["scenario_id"]["enum"]),
            sorted(mcp_tool_binding_harness.STARTER_SCENARIOS),
        )

    def test_bound_tool_evidence_keeps_tunnel_context_as_context_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            cell = self.cell_for(out, "benign_tool_call_bound")

            self.assertEqual(cell["claim_outcome"], "bound_tool_evidence")
            self.assertEqual(cell["transport_profile"], "mcp_tunnel_synthetic")
            self.assertEqual(
                cell["transport_context"]["transport_claim"],
                "transport_context_only",
            )
            self.assertEqual(
                cell["transport_context"]["connection_direction"],
                "outbound_only",
            )
            self.assertIn(
                "does_not_treat_tunnel_routing_as_tool_intent",
                cell["non_claims"],
            )
            self.assertIn(
                "does_not_claim_tunnel_authenticates_upstream_mcp_server",
                cell["non_claims"],
            )
            self.assertTrue((out / "benign_tool_call_bound" / "transport-context.json").exists())

    def test_description_drift_requires_visible_description_mismatch(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            cell = self.cell_for(out, "description_changed_before_call")

            self.assertEqual(cell["claim_outcome"], "description_drift")
            self.assertFalse(cell["description_matches_manifest"])
            self.assertNotEqual(
                cell["called_tool_manifest_digest"],
                cell["called_tool_description_digest"],
            )

    def test_effect_outside_boundary_keeps_maliciousness_out_of_claim(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            cell = self.cell_for(out, "effect_outside_declared_tool_boundary")

            self.assertEqual(
                cell["claim_outcome"],
                "effect_outside_declared_tool_boundary",
            )
            self.assertEqual(cell["measured_effect_kind"], "filesystem_write")
            self.assertFalse(cell["effect_within_declared_boundary"])
            self.assertIn("does_not_claim_root_cause", cell["non_claims"])
            self.assertIn("does_not_classify_malicious_intent", cell["non_claims"])

    def test_absence_and_no_effect_are_distinct_boundaries(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            visible_no_call = self.cell_for(out, "description_visible_no_call")
            no_effect = self.cell_for(out, "call_made_no_measurable_effect")

            self.assertFalse(visible_no_call["call_observed"])
            self.assertEqual(visible_no_call["claim_outcome"], "diagnostic_only")
            self.assertEqual(visible_no_call["effect_capture_status"], "unobserved")
            self.assertTrue(visible_no_call["required_links_complete"])
            self.assertTrue(no_effect["call_observed"])
            self.assertEqual(no_effect["claim_outcome"], "inconclusive")
            self.assertEqual(no_effect["effect_capture_status"], "unavailable")
            self.assertFalse(no_effect["required_links_complete"])

    def test_co_visible_context_preserves_plural_descriptions_without_causation(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            cell = self.cell_for(out, "call_made_with_other_descriptions_visible")
            context = load_json(
                out
                / "call_made_with_other_descriptions_visible"
                / "context-descriptor-set.json"
            )

            self.assertEqual(
                cell["claim_outcome"],
                "call_isolated_in_visible_context",
            )
            self.assertEqual(cell["called_tool_name"], "read_file")
            self.assertEqual(cell["co_visible_tool_names"], ["read_file", "write_file"])
            self.assertEqual(len(cell["model_visible_tool_description_refs"]), 2)
            self.assertEqual([tool["name"] for tool in context["tools"]], ["read_file", "write_file"])
            self.assertIn(
                "does_not_claim_co_visible_description_caused_call",
                cell["non_claims"],
            )

    def test_cli_generates_selected_scenario(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "runs"
            stdout = io.StringIO()

            with redirect_stdout(stdout):
                exit_code = mcp_tool_binding_harness.main(
                    [
                        "--out-dir",
                        str(out),
                        "--scenario",
                        "description_visible_no_call",
                        "--assay-commit",
                        "cli-test-commit",
                    ]
                )

            self.assertEqual(exit_code, 0)
            self.assertIn("description_visible_no_call", stdout.getvalue())
            self.assertEqual(
                sorted(path.name for path in out.iterdir()),
                ["description_visible_no_call"],
            )
            self.assertEqual(
                self.cell_for(out, "description_visible_no_call")["assay_commit"],
                "cli-test-commit",
            )

    def test_cli_rejects_non_empty_output_directory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "runs"
            out.mkdir()
            (out / "existing.txt").write_text("keep\n", encoding="utf-8")

            with self.assertRaises(SystemExit):
                mcp_tool_binding_harness.generate_harness(
                    out_dir=out,
                    scenarios=["benign_tool_call_bound"],
                    assay_commit="test",
                )

    def test_checked_in_starter_outputs_match_harness(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            generated = Path(tmp) / "starter-synthetic"
            mcp_tool_binding_harness.generate_harness(
                out_dir=generated,
                scenarios=list(mcp_tool_binding_harness.STARTER_SCENARIOS),
                assay_commit="synthetic-starter-output",
                created_at="2026-05-29T00:00:00Z",
            )

            checked_in_files = sorted(
                path.relative_to(STABLE_RUNS)
                for path in STABLE_RUNS.rglob("*")
                if path.is_file()
            )
            generated_files = sorted(
                path.relative_to(generated)
                for path in generated.rglob("*")
                if path.is_file()
            )

            self.assertEqual(generated_files, checked_in_files)
            for relative_path in checked_in_files:
                self.assertEqual(
                    (generated / relative_path).read_text(encoding="utf-8"),
                    (STABLE_RUNS / relative_path).read_text(encoding="utf-8"),
                    f"stable output drifted: {relative_path}",
                )


if __name__ == "__main__":
    unittest.main()
