#!/usr/bin/env python3
"""Generate synthetic interop coverage rows for the Slice 6 starter matrix."""

from __future__ import annotations

import argparse
import json
import subprocess
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from evidence_pack import utc_now

INTEROP_COVERAGE_CELL_SCHEMA = (
    "assay.experiment.agent_observability_fidelity.interop_coverage_cell.v0"
)
JOIN_RESULT_SCHEMA = "assay.observability.join_result.v0"
CLAIM_CLASS_CELL_SCHEMA = "assay.observability.claim_class_cell.v0"

STARTER_CELLS = (
    "single_tool_joined_all",
    "hidden_write_joined_all",
    "retry_temporal_partial",
    "runtime_surface_archive_only",
    "retrieval_then_tool_openinference",
)

OTEL_AGENT_SPANS_URL = (
    "https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-agent-spans/"
)
OTEL_CLIENT_SPANS_URL = "https://opentelemetry.io/docs/specs/semconv/gen-ai/gen-ai-spans/"
OPENINFERENCE_URL = (
    "https://arize-ai.github.io/openinference/spec/semantic_conventions.html"
)


@dataclass(frozen=True)
class InteropRow:
    cell_id: str
    scenario_id: str
    observation_profile: str
    agent_shape: str
    join_key: str
    evidence_layer: str
    coverage_status: str
    claim_strength: str
    claim_basis: str
    mapping: dict[str, str]
    mapping_basis: str
    mapping_notes: list[str]
    non_claims: list[str]
    join_result_ref: str | None = None
    otel_operation_name: str | None = None
    otel_semconv_opt_in: str | None = None
    openinference_span_kind: str | None = None
    runner_effect_kind: str | None = None


def referenced_join_result(
    row: InteropRow, join_results: list[dict[str, Any]]
) -> dict[str, Any] | None:
    if row.join_result_ref is None:
        return None
    prefix = "join-results.json#/"
    if not row.join_result_ref.startswith(prefix):
        raise ValueError(f"unsupported join_result_ref: {row.join_result_ref}")
    raw_index = row.join_result_ref[len(prefix) :]
    try:
        index = int(raw_index)
    except ValueError as exc:
        raise ValueError(
            f"unsupported join_result_ref (expected {prefix}<int>): {row.join_result_ref}"
        ) from exc
    if index < 0 or index >= len(join_results):
        raise ValueError(f"join_result_ref index out of range: {row.join_result_ref}")
    return join_results[index]


def row_joinability(row: InteropRow, join_results: list[dict[str, Any]]) -> str:
    if row.coverage_status == "absent" or row.mapping_basis == "not_expressible":
        return "not_joinable"
    join = referenced_join_result(row, join_results)
    if join is not None:
        join_grade = join["join_grade"]
        if join_grade == "strong":
            return "strong_join"
        if join_grade in {"weak", "diagnostic"}:
            return "diagnostic_join"
        if join_grade == "failed":
            return "not_joinable"
        raise ValueError(f"unsupported join_grade: {join_grade}")
    if row.join_key in {"run_id", "trace_span_id", "timestamp_or_order"}:
        return "diagnostic_join"
    return "not_applicable"


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def git_commit() -> str:
    try:
        return subprocess.check_output(
            ["git", "rev-parse", "HEAD"],
            cwd=Path(__file__).resolve().parents[3],
            text=True,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unknown"


def source_snapshot(
    observation_profile: str,
    *,
    assay_commit: str,
    retrieval_date: str,
) -> dict[str, Any]:
    if observation_profile == "openinference":
        return {
            "source_name": "OpenInference semantic conventions",
            "url": OPENINFERENCE_URL,
            "retrieval_date": retrieval_date,
            "version_anchor": {
                "kind": "package_version",
                "value": "openinference-semantic-conventions 0.1.1",
            },
        }
    if observation_profile == "runner_measured_effects":
        return {
            "source_name": "Assay synthetic Runner measured effects",
            "url": f"https://github.com/Rul1an/assay/tree/{assay_commit}",
            "retrieval_date": retrieval_date,
            "version_anchor": {
                "kind": "assay_commit",
                "value": assay_commit,
            },
        }
    return {
        "source_name": "OpenTelemetry GenAI semantic conventions",
        "url": OTEL_CLIENT_SPANS_URL
        if observation_profile == "otel_genai_latest_experimental"
        else OTEL_AGENT_SPANS_URL,
        "retrieval_date": retrieval_date,
        "version_anchor": {
            "kind": "semconv_tag",
            "value": "1.41.0",
        },
    }


def row_payload(
    row: InteropRow,
    *,
    index: int,
    assay_commit: str,
    retrieval_date: str,
    join_results: list[dict[str, Any]],
) -> dict[str, Any]:
    payload: dict[str, Any] = {
        "schema": INTEROP_COVERAGE_CELL_SCHEMA,
        "cell_id": row.cell_id,
        "scenario_id": row.scenario_id,
        "observation_profile": row.observation_profile,
        "source_snapshot": source_snapshot(
            row.observation_profile,
            assay_commit=assay_commit,
            retrieval_date=retrieval_date,
        ),
        "agent_shape": row.agent_shape,
        "join_key": row.join_key,
        "joinability": row_joinability(row, join_results),
        "evidence_layer": row.evidence_layer,
        "coverage_status": row.coverage_status,
        "claim_strength": row.claim_strength,
        "claim_basis": row.claim_basis,
        "mapping": row.mapping,
        "mapping_basis": row.mapping_basis,
        "mapping_notes": row.mapping_notes,
        "non_claims": row.non_claims,
        "claim_class_cell_ref": f"claim-class-cells.json#/{index}",
        "join_result_ref": row.join_result_ref,
    }
    if row.otel_operation_name is not None:
        payload["otel_operation_name"] = row.otel_operation_name
    if row.otel_semconv_opt_in is not None:
        payload["otel_semconv_opt_in"] = row.otel_semconv_opt_in
    if row.openinference_span_kind is not None:
        payload["openinference_span_kind"] = row.openinference_span_kind
    if row.runner_effect_kind is not None:
        payload["runner_effect_kind"] = row.runner_effect_kind
    return payload


def join_result(
    *,
    join_key: str,
    join_value: str | None,
    join_grade: str,
    scope: str,
    fallback_used: bool,
    evidence_refs: list[str],
    notes: list[str],
) -> dict[str, Any]:
    return {
        "schema": JOIN_RESULT_SCHEMA,
        "left_artifact_role": "otel_family_trace",
        "right_artifact_role": "measured_run_archive",
        "join_key": join_key,
        "join_value": join_value,
        "join_grade": join_grade,
        "scope": scope,
        "unique_within_scope": join_grade == "strong",
        "fallback_used": fallback_used,
        "evidence_refs": evidence_refs,
        "notes": notes,
    }


def claim_cell_for_row(row: dict[str, Any]) -> dict[str, Any]:
    if row["claim_strength"] == "absent":
        artifact_role = "none"
        evidence_refs: list[str] = []
    elif row["observation_profile"] == "runner_measured_effects":
        artifact_role = "measured_run_archive"
        evidence_refs = [f"interop-coverage-cells.json#/{row['_index']}"]
    elif row["evidence_layer"] == "joined":
        artifact_role = "joined_artifacts"
        evidence_refs = [row["join_result_ref"]]
    else:
        artifact_role = "otel_family_trace"
        evidence_refs = [f"interop-coverage-cells.json#/{row['_index']}"]
    return {
        "schema": CLAIM_CLASS_CELL_SCHEMA,
        "claim_type": row["mapping"].get("assay_claim_type", "interop_coverage"),
        "artifact_role": artifact_role,
        "claim_strength": row["claim_strength"],
        "claim_basis": row["claim_basis"],
        "evidence_refs": evidence_refs,
        "notes": row["mapping_notes"],
        "non_claims": row["non_claims"],
    }


def non_claims(*extra: str) -> list[str]:
    return [
        "does_not_rank_observability_products",
        "does_not_define_runtime_translator",
        "does_not_publish_delegated_interop_measurement",
        *extra,
    ]


def joined_tool_rows(
    *,
    cell_id: str,
    scenario_id: str,
    agent_shape: str,
    join_ref: str,
    runner_effect_kind: str,
    runner_value: str,
    trace_coverage: str,
    trace_strength: str,
    trace_basis: str,
    trace_notes: list[str],
) -> list[InteropRow]:
    return [
        InteropRow(
            cell_id=cell_id,
            scenario_id=scenario_id,
            observation_profile="otel_genai_default",
            agent_shape=agent_shape,
            join_key="tool_call_id",
            evidence_layer="joined",
            coverage_status=trace_coverage,
            claim_strength=trace_strength,
            claim_basis="reported",
            mapping={
                "otel_field": "gen_ai.operation.name",
                "otel_value": "execute_tool",
                "assay_claim_type": "reported_tool_intent",
            },
            mapping_basis=trace_basis,
            mapping_notes=trace_notes,
            non_claims=non_claims("does_not_claim_semantic_equivalence"),
            join_result_ref=join_ref,
            otel_operation_name="execute_tool",
            otel_semconv_opt_in="none",
        ),
        InteropRow(
            cell_id=cell_id,
            scenario_id=scenario_id,
            observation_profile="openinference",
            agent_shape=agent_shape,
            join_key="tool_call_id",
            evidence_layer="joined",
            coverage_status=trace_coverage,
            claim_strength=trace_strength,
            claim_basis="reported",
            mapping={
                "openinference_field": "openinference.span.kind",
                "openinference_value": "TOOL",
                "assay_claim_type": "reported_tool_intent",
            },
            mapping_basis=trace_basis,
            mapping_notes=trace_notes,
            non_claims=non_claims("does_not_claim_semantic_equivalence"),
            join_result_ref=join_ref,
            openinference_span_kind="TOOL",
        ),
        InteropRow(
            cell_id=cell_id,
            scenario_id=scenario_id,
            observation_profile="runner_measured_effects",
            agent_shape=agent_shape,
            join_key="tool_call_id",
            evidence_layer="joined",
            coverage_status="full",
            claim_strength="strong",
            claim_basis="measured",
            mapping={
                "runner_field": "effects[].effect",
                "runner_value": runner_value,
                "assay_claim_type": "measured_runner_effect",
            },
            mapping_basis="synthetic_fixture",
            mapping_notes=[
                "Runner row records the measured synthetic effect only; tool intent comes from the joined trace layer."
            ],
            non_claims=non_claims("does_not_infer_tool_intent_from_runner_alone"),
            join_result_ref=join_ref,
            runner_effect_kind=runner_effect_kind,
        ),
    ]


def cell_definitions() -> dict[str, dict[str, Any]]:
    return {
        "single_tool_joined_all": {
            "scenario_id": "matched_safe_read",
            "join_results": [
                join_result(
                    join_key="tool_call_id",
                    join_value="tc_semantic_gap_matched_safe_read",
                    join_grade="strong",
                    scope="tool_call",
                    fallback_used=False,
                    evidence_refs=[
                        "synthetic-trace.json#/tool_calls/0",
                        "synthetic-runner-archive.json#/effects/0",
                    ],
                    notes=[
                        "Synthetic baseline joins reported tool execution to measured filesystem read by tool_call_id."
                    ],
                )
            ],
            "rows": joined_tool_rows(
                cell_id="single_tool_joined_all",
                scenario_id="matched_safe_read",
                agent_shape="single_tool_call",
                join_ref="join-results.json#/0",
                runner_effect_kind="filesystem_read",
                runner_value="read",
                trace_coverage="full",
                trace_strength="strong",
                trace_basis="explicit_upstream_doc",
                trace_notes=[
                    "Tool execution is expressible as OTel execute_tool or OpenInference TOOL span kind.",
                    "The shared synthetic tool_call_id supplies the join; the vocabulary row does not rank either trace format.",
                ],
            ),
        },
        "hidden_write_joined_all": {
            "scenario_id": "hidden_write",
            "join_results": [
                join_result(
                    join_key="tool_call_id",
                    join_value="tc_semantic_gap_hidden_write",
                    join_grade="strong",
                    scope="tool_call",
                    fallback_used=False,
                    evidence_refs=[
                        "synthetic-trace.json#/tool_calls/0",
                        "synthetic-runner-archive.json#/effects/0",
                    ],
                    notes=[
                        "Synthetic trace and measured write share the tool_call_id, but the trace vocabulary does not carry the measured filesystem write."
                    ],
                )
            ],
            "rows": joined_tool_rows(
                cell_id="hidden_write_joined_all",
                scenario_id="hidden_write",
                agent_shape="single_tool_call",
                join_ref="join-results.json#/0",
                runner_effect_kind="filesystem_write",
                runner_value="create_write",
                trace_coverage="partial",
                trace_strength="partial",
                trace_basis="derived_join_rule",
                trace_notes=[
                    "The trace row can express tool execution intent, but the measured write is only visible after joining with Runner effects.",
                    "Partial coverage is a semantic-gap boundary, not a vocabulary failure ranking.",
                ],
            ),
        },
        "retry_temporal_partial": {
            "scenario_id": "retry_self_correction",
            "join_results": [
                join_result(
                    join_key="tool_call_id",
                    join_value="tc_semantic_gap_retry_self_correction",
                    join_grade="strong",
                    scope="tool_call",
                    fallback_used=False,
                    evidence_refs=[
                        "synthetic-trace.json#/tool_calls/0",
                        "synthetic-runner-archive.json#/effects",
                    ],
                    notes=[
                        "Synthetic trace reports terminal success while Runner effects preserve the failed attempts and final read."
                    ],
                )
            ],
            "rows": joined_tool_rows(
                cell_id="retry_temporal_partial",
                scenario_id="retry_self_correction",
                agent_shape="retry_self_correction",
                join_ref="join-results.json#/0",
                runner_effect_kind="filesystem_read",
                runner_value="failed_open,failed_open,read",
                trace_coverage="partial",
                trace_strength="partial",
                trace_basis="derived_join_rule",
                trace_notes=[
                    "Terminal tool-success coverage is partial because the joined archive carries prior failed attempts.",
                    "The row does not claim retry behavior is bad.",
                ],
            ),
        },
        "runtime_surface_archive_only": {
            "scenario_id": "runtime_side_effect",
            "join_results": [
                join_result(
                    join_key="run_id",
                    join_value="synthetic_runtime_side_effect",
                    join_grade="diagnostic",
                    scope="run",
                    fallback_used=False,
                    evidence_refs=["synthetic-runner-archive.json#/effects/0"],
                    notes=[
                        "Runtime probe is run-scope measured evidence and is not upgraded to tool intent."
                    ],
                )
            ],
            "rows": [
                InteropRow(
                    cell_id="runtime_surface_archive_only",
                    scenario_id="runtime_side_effect",
                    observation_profile="runner_measured_effects",
                    agent_shape="runtime_side_effect",
                    join_key="run_id",
                    evidence_layer="archive_only",
                    coverage_status="full",
                    claim_strength="strong",
                    claim_basis="measured",
                    mapping={
                        "runner_field": "effects[].effect",
                        "runner_value": "runtime_config_probe",
                        "assay_claim_type": "measured_runtime_effect",
                    },
                    mapping_basis="synthetic_fixture",
                    mapping_notes=[
                        "Runner measures a runtime probe, but the row stays archive-only."
                    ],
                    non_claims=non_claims(
                        "does_not_infer_tool_intent_from_runner_alone",
                        "does_not_attribute_runtime_effect_to_tool_intent"
                    ),
                    runner_effect_kind="runtime_probe",
                ),
                InteropRow(
                    cell_id="runtime_surface_archive_only",
                    scenario_id="runtime_side_effect",
                    observation_profile="otel_genai_default",
                    agent_shape="runtime_side_effect",
                    join_key="run_id",
                    evidence_layer="trace_only",
                    coverage_status="absent",
                    claim_strength="absent",
                    claim_basis="reported",
                    mapping={
                        "assay_claim_type": "measured_runtime_effect",
                    },
                    mapping_basis="not_expressible",
                    mapping_notes=[
                        "No OTel GenAI trace field expresses the measured runtime filesystem probe itself."
                    ],
                    non_claims=non_claims("does_not_claim_absent_behavior"),
                    otel_semconv_opt_in="none",
                ),
                InteropRow(
                    cell_id="runtime_surface_archive_only",
                    scenario_id="runtime_side_effect",
                    observation_profile="openinference",
                    agent_shape="runtime_side_effect",
                    join_key="run_id",
                    evidence_layer="trace_only",
                    coverage_status="absent",
                    claim_strength="absent",
                    claim_basis="reported",
                    mapping={
                        "assay_claim_type": "measured_runtime_effect",
                    },
                    mapping_basis="not_expressible",
                    mapping_notes=[
                        "OpenInference span kind vocabulary does not express a measured runtime probe without a trace span."
                    ],
                    non_claims=non_claims("does_not_claim_absent_behavior"),
                ),
            ],
        },
        "retrieval_then_tool_openinference": {
            "scenario_id": "retrieval_then_tool",
            "join_results": [
                join_result(
                    join_key="trace_span_id",
                    join_value="span_retrieval_then_tool",
                    join_grade="weak",
                    scope="trace_local",
                    fallback_used=False,
                    evidence_refs=["synthetic-trace.json#/spans"],
                    notes=[
                        "Retrieval/tool mix is trace-local synthetic evidence; Runner is not used to infer retrieval semantics."
                    ],
                )
            ],
            "rows": [
                InteropRow(
                    cell_id="retrieval_then_tool_openinference",
                    scenario_id="retrieval_then_tool",
                    observation_profile="openinference",
                    agent_shape="retrieval_then_tool",
                    join_key="trace_span_id",
                    evidence_layer="trace_only",
                    coverage_status="full",
                    claim_strength="strong",
                    claim_basis="reported",
                    mapping={
                        "openinference_field": "openinference.span.kind",
                        "openinference_value": "RETRIEVER",
                        "assay_claim_type": "reported_retrieval_step",
                    },
                    mapping_basis="explicit_upstream_doc",
                    mapping_notes=[
                        "OpenInference has a dedicated RETRIEVER span kind for retrieval steps."
                    ],
                    non_claims=non_claims("does_not_claim_semantic_equivalence"),
                    openinference_span_kind="RETRIEVER",
                ),
                InteropRow(
                    cell_id="retrieval_then_tool_openinference",
                    scenario_id="retrieval_then_tool",
                    observation_profile="otel_genai_latest_experimental",
                    agent_shape="retrieval_then_tool",
                    join_key="trace_span_id",
                    evidence_layer="trace_only",
                    coverage_status="partial",
                    claim_strength="partial",
                    claim_basis="reported",
                    mapping={
                        "otel_field": "gen_ai.operation.name",
                        "otel_value": "retrieval",
                        "assay_claim_type": "reported_retrieval_step",
                    },
                    mapping_basis="explicit_upstream_doc",
                    mapping_notes=[
                        "OTel latest experimental GenAI conventions include a retrieval operation, but this starter row does not claim equivalence with OpenInference RETRIEVER spans."
                    ],
                    non_claims=non_claims("does_not_claim_semantic_equivalence"),
                    otel_operation_name="retrieval",
                    otel_semconv_opt_in="gen_ai_latest_experimental",
                ),
                InteropRow(
                    cell_id="retrieval_then_tool_openinference",
                    scenario_id="retrieval_then_tool",
                    observation_profile="runner_measured_effects",
                    agent_shape="retrieval_then_tool",
                    join_key="trace_span_id",
                    evidence_layer="archive_only",
                    coverage_status="absent",
                    claim_strength="absent",
                    claim_basis="measured",
                    mapping={
                        "assay_claim_type": "reported_retrieval_step",
                    },
                    mapping_basis="not_expressible",
                    mapping_notes=[
                        "Runner measured effects must not infer retrieval semantics without trace or receipt evidence."
                    ],
                    non_claims=non_claims(
                        "does_not_infer_retrieval_semantics_from_runner_alone"
                    ),
                ),
            ],
        },
    }


def render_summary(*, cell_id: str, rows: list[dict[str, Any]]) -> str:
    lines = [
        f"# Interop Coverage Cell: {cell_id}",
        "",
        "| Observation profile | Coverage | Joinability | Claim strength | Mapping basis |",
        "|---|---|---|---|---|",
    ]
    for row in rows:
        lines.append(
            f"| `{row['observation_profile']}` | `{row['coverage_status']}` | "
            f"`{row['joinability']}` | `{row['claim_strength']}` | "
            f"`{row['mapping_basis']}` |"
        )
    lines.extend(
        [
            "",
            "## Non-Claims",
            "",
            "- does_not_rank_observability_products",
            "- does_not_define_runtime_translator",
            "- does_not_publish_delegated_interop_measurement",
            "",
        ]
    )
    return "\n".join(lines)


def generate_cell(
    *,
    cell_id: str,
    root: Path,
    assay_commit: str,
    retrieval_date: str,
) -> Path:
    definitions = cell_definitions()
    definition = definitions[cell_id]
    cell_dir = root / cell_id
    if cell_dir.exists() and any(cell_dir.iterdir()):
        raise FileExistsError(f"cell output directory is not empty: {cell_dir}")
    cell_dir.mkdir(parents=True, exist_ok=True)

    rows = [
        row_payload(
            row,
            index=index,
            assay_commit=assay_commit,
            retrieval_date=retrieval_date,
            join_results=definition["join_results"],
        )
        for index, row in enumerate(definition["rows"])
    ]
    rows_for_claims = []
    for index, row in enumerate(rows):
        row_for_claim = dict(row)
        row_for_claim["_index"] = index
        rows_for_claims.append(row_for_claim)
    claim_cells = [claim_cell_for_row(row) for row in rows_for_claims]

    write_json(cell_dir / "interop-coverage-cells.json", rows)
    write_json(cell_dir / "join-results.json", definition["join_results"])
    write_json(cell_dir / "claim-class-cells.json", claim_cells)
    (cell_dir / "summary.md").write_text(
        render_summary(cell_id=cell_id, rows=rows),
        encoding="utf-8",
    )
    return cell_dir


def generate_harness(
    *,
    out_dir: Path,
    cells: list[str],
    assay_commit: str,
    retrieval_date: str,
) -> list[Path]:
    if out_dir.exists() and any(out_dir.iterdir()):
        raise FileExistsError(f"output directory is not empty: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)
    definitions = cell_definitions()
    generated = []
    for cell_id in cells:
        if cell_id not in definitions:
            raise KeyError(f"unknown interop cell: {cell_id}")
        generated.append(
            generate_cell(
                cell_id=cell_id,
                root=out_dir,
                assay_commit=assay_commit,
                retrieval_date=retrieval_date,
            )
        )
    return generated


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument(
        "--cell",
        action="append",
        choices=STARTER_CELLS,
        help="Interop starter cell to generate. May be repeated. Defaults to all starter cells.",
    )
    parser.add_argument("--assay-commit", default=git_commit())
    parser.add_argument("--retrieval-date", default=utc_now().split("T", 1)[0])
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    cells = args.cell or list(STARTER_CELLS)
    generated = generate_harness(
        out_dir=args.out_dir,
        cells=cells,
        assay_commit=args.assay_commit,
        retrieval_date=args.retrieval_date,
    )
    for path in generated:
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
