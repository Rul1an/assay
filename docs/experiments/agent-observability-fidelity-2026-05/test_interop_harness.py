"""Tests for the synthetic interop matrix harness."""

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
HARNESS_PATH = ROOT / "interop_harness.py"
OBSERVABILITY_SCHEMA_ROOT = (
    ROOT.parent.parent / "reference" / "observability" / "schema"
)

spec = importlib.util.spec_from_file_location("interop_harness", HARNESS_PATH)
assert spec is not None and spec.loader is not None
interop_harness = importlib.util.module_from_spec(spec)
sys.modules["interop_harness"] = interop_harness
spec.loader.exec_module(interop_harness)


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def load_schema(path: Path) -> dict[str, Any]:
    return json.loads(path.read_text(encoding="utf-8"))


class InteropHarnessTests(unittest.TestCase):
    def generate(self, root: Path) -> Path:
        out = root / "interop-runs"
        interop_harness.generate_harness(
            out_dir=out,
            cells=list(interop_harness.STARTER_CELLS),
            assay_commit="test-assay-commit",
            retrieval_date="2026-05-28",
        )
        return out

    def rows_for(self, out: Path, cell_id: str) -> list[dict[str, Any]]:
        return load_json(out / cell_id / "interop-coverage-cells.json")

    def test_harness_emits_all_starter_cells(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))

            self.assertEqual(
                sorted(path.name for path in out.iterdir()),
                sorted(interop_harness.STARTER_CELLS),
            )
            for cell_id in interop_harness.STARTER_CELLS:
                cell_dir = out / cell_id
                for name in (
                    "interop-coverage-cells.json",
                    "join-results.json",
                    "claim-class-cells.json",
                    "summary.md",
                ):
                    self.assertTrue((cell_dir / name).exists(), name)

    def test_single_tool_cell_has_all_boundary_tool_call_join(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            rows = self.rows_for(out, "single_tool_joined_all")
            joins = load_json(out / "single_tool_joined_all" / "join-results.json")

            self.assertEqual(
                {row["observation_profile"] for row in rows},
                {
                    "otel_genai_default",
                    "openinference",
                    "runner_measured_effects",
                },
            )
            self.assertEqual(joins[0]["join_key"], "tool_call_id")
            self.assertEqual(joins[0]["join_grade"], "strong")
            for row in rows:
                self.assertEqual(row["coverage_status"], "full")
                self.assertEqual(row["claim_strength"], "strong")
                self.assertEqual(row["evidence_layer"], "joined")
                self.assertEqual(row["join_result_ref"], "join-results.json#/0")

    def test_partial_and_absent_rows_are_first_class_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            rows = [
                row
                for cell_id in interop_harness.STARTER_CELLS
                for row in self.rows_for(out, cell_id)
            ]

            self.assertIn("partial", {row["coverage_status"] for row in rows})
            self.assertIn("absent", {row["coverage_status"] for row in rows})
            absent = [row for row in rows if row["coverage_status"] == "absent"]
            self.assertTrue(absent)
            for row in absent:
                self.assertEqual(row["claim_strength"], "absent")
                self.assertEqual(row["mapping_basis"], "not_expressible")
                self.assertIsNone(row["join_result_ref"])

    def test_latest_otel_row_records_exact_opt_in(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            rows = self.rows_for(out, "retrieval_then_tool_openinference")
            latest = [
                row
                for row in rows
                if row["observation_profile"] == "otel_genai_latest_experimental"
            ]

            self.assertEqual(len(latest), 1)
            self.assertEqual(
                latest[0]["otel_semconv_opt_in"], "gen_ai_latest_experimental"
            )
            self.assertEqual(latest[0]["otel_operation_name"], "retrieval")
            self.assertEqual(
                latest[0]["source_snapshot"]["version_anchor"],
                {"kind": "semconv_tag", "value": "1.41.0"},
            )

    def test_openinference_rows_record_span_kind_when_expressible(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            rows = [
                row
                for cell_id in interop_harness.STARTER_CELLS
                for row in self.rows_for(out, cell_id)
                if row["observation_profile"] == "openinference"
                and row["coverage_status"] in {"full", "partial"}
            ]

            self.assertTrue(rows)
            self.assertIn("TOOL", {row["openinference_span_kind"] for row in rows})
            self.assertIn(
                "RETRIEVER", {row["openinference_span_kind"] for row in rows}
            )
            for row in rows:
                self.assertEqual(
                    row["source_snapshot"]["version_anchor"],
                    {
                        "kind": "package_version",
                        "value": "openinference-semantic-conventions 0.1.1",
                    },
                )

    def test_runner_rows_stay_measured_effects_only(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            rows = [
                row
                for cell_id in interop_harness.STARTER_CELLS
                for row in self.rows_for(out, cell_id)
                if row["observation_profile"] == "runner_measured_effects"
            ]

            self.assertTrue(rows)
            for row in rows:
                if row["coverage_status"] != "absent":
                    self.assertEqual(row["claim_basis"], "measured")
                    self.assertEqual(row["mapping_basis"], "synthetic_fixture")
                    self.assertIn("runner_effect_kind", row)
                self.assertIn(
                    "does_not_infer",
                    " ".join(row["non_claims"]),
                )

    def test_outputs_match_pinned_schemas(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            interop_schema = load_schema(
                ROOT / "schema" / "interop-coverage-cell-v0.schema.json"
            )
            join_schema = load_schema(
                OBSERVABILITY_SCHEMA_ROOT / "join-result-v0.schema.json"
            )
            claim_schema = load_schema(
                OBSERVABILITY_SCHEMA_ROOT / "claim-class-cell-v0.schema.json"
            )

            for cell_id in interop_harness.STARTER_CELLS:
                cell_dir = out / cell_id
                rows = load_json(cell_dir / "interop-coverage-cells.json")
                joins = load_json(cell_dir / "join-results.json")
                cells = load_json(cell_dir / "claim-class-cells.json")
                for index, row in enumerate(rows):
                    assert_matches_schema(
                        self,
                        row,
                        interop_schema,
                        root=interop_schema,
                        path=f"$[{cell_id}][{index}]",
                    )
                for index, join in enumerate(joins):
                    assert_matches_schema(
                        self,
                        join,
                        join_schema,
                        root=join_schema,
                        path=f"$[{cell_id}].joins[{index}]",
                    )
                for index, cell in enumerate(cells):
                    assert_matches_schema(
                        self,
                        cell,
                        claim_schema,
                        root=claim_schema,
                        path=f"$[{cell_id}].claim_cells[{index}]",
                    )

    def test_joined_rows_reference_join_result_and_claim_class_cell(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = self.generate(Path(tmp))
            for cell_id in interop_harness.STARTER_CELLS:
                rows = self.rows_for(out, cell_id)
                for index, row in enumerate(rows):
                    self.assertEqual(
                        row["claim_class_cell_ref"], f"claim-class-cells.json#/{index}"
                    )
                    if row["evidence_layer"] == "joined":
                        self.assertRegex(row["join_result_ref"], r"^join-results.json#/")

    def test_schema_enums_match_harness_constants(self) -> None:
        schema = load_schema(ROOT / "schema" / "interop-coverage-cell-v0.schema.json")

        self.assertEqual(
            sorted(schema["properties"]["cell_id"]["enum"]),
            sorted(interop_harness.STARTER_CELLS),
        )
        self.assertEqual(
            sorted(schema["properties"]["observation_profile"]["enum"]),
            [
                "openinference",
                "otel_genai_default",
                "otel_genai_latest_experimental",
                "runner_measured_effects",
            ],
        )

    def test_cli_generates_selected_cell(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "interop-runs"
            stdout = io.StringIO()

            with redirect_stdout(stdout):
                exit_code = interop_harness.main(
                    [
                        "--out-dir",
                        str(out),
                        "--cell",
                        "single_tool_joined_all",
                        "--assay-commit",
                        "test-assay-commit",
                        "--retrieval-date",
                        "2026-05-28",
                    ]
                )

            self.assertEqual(exit_code, 0)
            self.assertIn("single_tool_joined_all", stdout.getvalue())
            self.assertTrue(
                (out / "single_tool_joined_all" / "interop-coverage-cells.json").exists()
            )
            self.assertFalse((out / "hidden_write_joined_all").exists())

    def test_cli_generates_multiple_selected_cells(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "interop-runs"

            exit_code = interop_harness.main(
                [
                    "--out-dir",
                    str(out),
                    "--cell",
                    "single_tool_joined_all",
                    "--cell",
                    "hidden_write_joined_all",
                    "--assay-commit",
                    "test-assay-commit",
                    "--retrieval-date",
                    "2026-05-28",
                ]
            )

            self.assertEqual(exit_code, 0)
            self.assertTrue(
                (out / "single_tool_joined_all" / "interop-coverage-cells.json").exists()
            )
            self.assertTrue(
                (out / "hidden_write_joined_all" / "interop-coverage-cells.json").exists()
            )
            self.assertFalse((out / "retry_temporal_partial").exists())

    def test_existing_nonempty_output_directory_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp) / "interop-runs"
            out.mkdir()
            (out / "stale.txt").write_text("stale\n", encoding="utf-8")

            with self.assertRaises(FileExistsError):
                interop_harness.generate_harness(
                    out_dir=out,
                    cells=["single_tool_joined_all"],
                    assay_commit="test-assay-commit",
                    retrieval_date="2026-05-28",
                )


if __name__ == "__main__":
    unittest.main()
