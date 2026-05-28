#!/usr/bin/env python3
"""Generate synthetic semantic-gap harness outputs for the MVP scenarios."""

from __future__ import annotations

import argparse
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

from evidence_pack import build_pack, utc_now

JOIN_RESULT_SCHEMA = "assay.observability.join_result.v0"
CLAIM_CLASS_CELL_SCHEMA = "assay.observability.claim_class_cell.v0"
SEMANTIC_GAP_VERDICT_SCHEMA = (
    "assay.experiment.agent_observability_fidelity.semantic_gap_verdict.v0"
)
SYNTHETIC_TRACE_SCHEMA = (
    "assay.experiment.agent_observability_fidelity.synthetic_trace.v0"
)
SYNTHETIC_RUNNER_ARCHIVE_SCHEMA = (
    "assay.experiment.agent_observability_fidelity.synthetic_runner_archive.v0"
)

MVP_SCENARIOS = ("matched_safe_read", "hidden_write", "weak_join_fallback")


@dataclass(frozen=True)
class Scenario:
    scenario_id: str
    role: str
    verdict: str
    evidence_claim_class: str
    claim_summary: str
    trace_tool_call: dict[str, Any]
    measured_effect: dict[str, Any]
    join: dict[str, Any]
    joined_claim_strength: str
    joined_claim_basis: str
    joined_notes: list[str]
    non_claims: list[str]


def write_json(path: Path, payload: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(payload, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def clean_health() -> dict[str, Any]:
    return {
        "kernel_layer": "complete",
        "ringbuf_drops": 0,
        "cgroup_correlation": "clean",
    }


def scenario_definitions() -> dict[str, Scenario]:
    matched_tool_call_id = "tc_semantic_gap_matched_safe_read"
    hidden_tool_call_id = "tc_semantic_gap_hidden_write"
    return {
        "matched_safe_read": Scenario(
            scenario_id="matched_safe_read",
            role="baseline",
            verdict="positive_join",
            evidence_claim_class="positive_join",
            claim_summary=(
                "Synthetic trace intent and measured archive effect agree for "
                "safe.txt under a unique tool_call_id."
            ),
            trace_tool_call={
                "tool_call_id": matched_tool_call_id,
                "tool_name": "read_file",
                "reported_action": "read",
                "reported_path": "safe.txt",
                "order": 1,
            },
            measured_effect={
                "tool_call_id": matched_tool_call_id,
                "operation": "open",
                "effect": "read",
                "path": "safe.txt",
                "order": 1,
            },
            join={
                "join_key": "tool_call_id",
                "join_value": matched_tool_call_id,
                "join_grade": "strong",
                "scope": "tool_call",
                "unique_within_scope": True,
                "fallback_used": False,
                "notes": [
                    "Synthetic baseline: trace and archive name the same safe file."
                ],
            },
            joined_claim_strength="strong",
            joined_claim_basis="derived",
            joined_notes=[
                "Reported read intent and measured read effect agree inside the fixture boundary."
            ],
            non_claims=[
                "does_not_publish_delegated_gap_finding",
                "does_not_replace_runner_archive_integrity",
            ],
        ),
        "hidden_write": Scenario(
            scenario_id="hidden_write",
            role="gap",
            verdict="semantic_gap",
            evidence_claim_class="semantic_gap",
            claim_summary=(
                "Synthetic trace reports a read-only action while measured effects "
                "include a write in the same workdir and tool_call_id scope."
            ),
            trace_tool_call={
                "tool_call_id": hidden_tool_call_id,
                "tool_name": "read_file",
                "reported_action": "read",
                "reported_path": "safe.txt",
                "order": 1,
            },
            measured_effect={
                "tool_call_id": hidden_tool_call_id,
                "operation": "write",
                "effect": "create_write",
                "path": "side-effect.txt",
                "order": 1,
            },
            join={
                "join_key": "tool_call_id",
                "join_value": hidden_tool_call_id,
                "join_grade": "strong",
                "scope": "tool_call",
                "unique_within_scope": True,
                "fallback_used": False,
                "notes": [
                    "Same synthetic tool_call_id joins reported read intent to measured write effect."
                ],
            },
            joined_claim_strength="strong",
            joined_claim_basis="derived",
            joined_notes=[
                "Measured write effect diverges from reported read-only trace intent.",
                "Divergence is not classified as malicious behavior or root cause.",
            ],
            non_claims=[
                "does_not_claim_malicious_behavior",
                "does_not_claim_policy_failure",
                "does_not_publish_delegated_gap_finding",
            ],
        ),
        "weak_join_fallback": Scenario(
            scenario_id="weak_join_fallback",
            role="fallback",
            verdict="diagnostic_only",
            evidence_claim_class="diagnostic",
            claim_summary=(
                "Synthetic trace and measured effect are only order-adjacent; "
                "without a tool_call_id, the harness emits diagnostic correlation only."
            ),
            trace_tool_call={
                "tool_call_id": None,
                "tool_name": "read_file",
                "reported_action": "read",
                "reported_path": "safe.txt",
                "order": 1,
            },
            measured_effect={
                "tool_call_id": None,
                "operation": "open",
                "effect": "read",
                "path": "safe.txt",
                "order": 1,
            },
            join={
                "join_key": "timestamp_or_order",
                "join_value": "order:1",
                "join_grade": "diagnostic",
                "scope": "diagnostic",
                "unique_within_scope": False,
                "fallback_used": True,
                "notes": [
                    "No tool_call_id is present; order proximity is diagnostic only.",
                    "ambiguous_proximity",
                ],
            },
            joined_claim_strength="weak",
            joined_claim_basis="inferred",
            joined_notes=[
                "Order proximity supports investigation but not semantic equality.",
                "Fallback joins must not be upgraded to strong tool-call joins.",
            ],
            non_claims=[
                "does_not_support_semantic_equality",
                "does_not_publish_delegated_gap_finding",
                "does_not_replace_runner_archive_integrity",
            ],
        ),
    }


def trace_payload(scenario: Scenario) -> dict[str, Any]:
    return {
        "schema": SYNTHETIC_TRACE_SCHEMA,
        "scenario_id": scenario.scenario_id,
        "run_id": f"synthetic_{scenario.scenario_id}",
        "tool_calls": [scenario.trace_tool_call],
        "trace_calibration_status": "clean",
    }


def runner_archive_payload(scenario: Scenario) -> dict[str, Any]:
    return {
        "schema": SYNTHETIC_RUNNER_ARCHIVE_SCHEMA,
        "scenario_id": scenario.scenario_id,
        "run_id": f"synthetic_{scenario.scenario_id}",
        "effects": [scenario.measured_effect],
        "runner_health": clean_health(),
    }


def join_result(scenario: Scenario) -> dict[str, Any]:
    return {
        "schema": JOIN_RESULT_SCHEMA,
        "left_artifact_role": "otel_family_trace",
        "right_artifact_role": "measured_run_archive",
        "join_key": scenario.join["join_key"],
        "join_value": scenario.join["join_value"],
        "join_grade": scenario.join["join_grade"],
        "scope": scenario.join["scope"],
        "unique_within_scope": scenario.join["unique_within_scope"],
        "fallback_used": scenario.join["fallback_used"],
        "evidence_refs": [
            "trace.json#/tool_calls/0",
            "runner-archive.json#/effects/0",
        ],
        "notes": scenario.join["notes"],
    }


def claim_cell(
    *,
    claim_type: str,
    artifact_role: str,
    claim_strength: str,
    claim_basis: str,
    evidence_refs: list[str],
    notes: list[str],
    non_claims: list[str],
) -> dict[str, Any]:
    return {
        "schema": CLAIM_CLASS_CELL_SCHEMA,
        "claim_type": claim_type,
        "artifact_role": artifact_role,
        "claim_strength": claim_strength,
        "claim_basis": claim_basis,
        "evidence_refs": evidence_refs,
        "notes": notes,
        "non_claims": non_claims,
    }


def claim_cells(scenario: Scenario) -> list[dict[str, Any]]:
    trace_strength = (
        "partial" if scenario.scenario_id == "weak_join_fallback" else "strong"
    )
    return [
        claim_cell(
            claim_type="reported_trace_intent",
            artifact_role="otel_family_trace",
            claim_strength=trace_strength,
            claim_basis="reported",
            evidence_refs=["trace.json#/tool_calls/0"],
            notes=[
                "Trace layer reports the tool intent within the synthetic fixture boundary."
            ],
            non_claims=["does_not_prove_absence_of_unreported_effects"],
        ),
        claim_cell(
            claim_type="measured_runner_effect",
            artifact_role="measured_run_archive",
            claim_strength="strong",
            claim_basis="measured",
            evidence_refs=["runner-archive.json#/effects/0"],
            notes=[
                "Synthetic Runner archive layer records the measured system effect with clean health."
            ],
            non_claims=["does_not_verify_runner_archive_integrity"],
        ),
        claim_cell(
            claim_type=f"{scenario.verdict}_comparison",
            artifact_role="joined_artifacts",
            claim_strength=scenario.joined_claim_strength,
            claim_basis=scenario.joined_claim_basis,
            evidence_refs=["join-result.json"],
            notes=scenario.joined_notes,
            non_claims=scenario.non_claims,
        ),
    ]


def scenario_verdict(scenario: Scenario) -> dict[str, Any]:
    return {
        "schema": SEMANTIC_GAP_VERDICT_SCHEMA,
        "scenario_id": scenario.scenario_id,
        "role": scenario.role,
        "verdict": scenario.verdict,
        "evidence_pack_claim_class": scenario.evidence_claim_class,
        "runner_health_status": "clean",
        "trace_calibration_status": "clean",
        "join_key": scenario.join["join_key"],
        "join_grade": scenario.join["join_grade"],
        "fallback_used": scenario.join["fallback_used"],
        "reason": scenario.claim_summary,
        "non_claims": scenario.non_claims,
    }


def render_summary(
    *, scenario: Scenario, join: dict[str, Any], verdict: dict[str, Any]
) -> str:
    lines = [
        f"# Semantic Gap Scenario: {scenario.scenario_id}",
        "",
        "| Field | Value |",
        "|---|---|",
        f"| Role | `{scenario.role}` |",
        f"| Verdict | `{scenario.verdict}` |",
        f"| Evidence-pack claim class | `{scenario.evidence_claim_class}` |",
        f"| Join key | `{join['join_key']}` |",
        f"| Join grade | `{join['join_grade']}` |",
        f"| Fallback used | `{join['fallback_used']}` |",
        f"| Runner health | `{verdict['runner_health_status']}` |",
        f"| Trace calibration | `{verdict['trace_calibration_status']}` |",
        f"| Reason | {scenario.claim_summary} |",
        "",
        "## Non-Claims",
        "",
    ]
    for non_claim in scenario.non_claims:
        lines.append(f"- {non_claim}")
    lines.append("")
    return "\n".join(lines)


def generate_scenario(
    *,
    scenario: Scenario,
    root: Path,
    created_at: str,
    redaction_policy: str,
) -> Path:
    scenario_dir = root / scenario.scenario_id
    if scenario_dir.exists() and any(scenario_dir.iterdir()):
        raise FileExistsError(
            f"scenario output directory is not empty: {scenario_dir}"
        )
    scenario_dir.mkdir(parents=True, exist_ok=True)

    trace_path = scenario_dir / "trace.json"
    archive_path = scenario_dir / "runner-archive.json"
    health_path = scenario_dir / "observation-health.json"
    join_path = scenario_dir / "join-result.json"
    cells_path = scenario_dir / "claim-class-cells.json"
    verdict_path = scenario_dir / "scenario-verdict.json"

    trace = trace_payload(scenario)
    archive = runner_archive_payload(scenario)
    health = clean_health()
    join = join_result(scenario)
    cells = claim_cells(scenario)
    verdict = scenario_verdict(scenario)

    write_json(trace_path, trace)
    write_json(archive_path, archive)
    write_json(health_path, health)
    write_json(join_path, join)
    write_json(cells_path, cells)
    write_json(verdict_path, verdict)
    (scenario_dir / "summary.md").write_text(
        render_summary(scenario=scenario, join=join, verdict=verdict),
        encoding="utf-8",
    )

    build_pack(
        out_dir=scenario_dir / "evidence-pack",
        scenario_id=scenario.scenario_id,
        claim_summary=scenario.claim_summary,
        claim_class=scenario.evidence_claim_class,
        runner_archive=archive_path,
        trace_json=trace_path,
        observation_health=health_path,
        created_at=created_at,
        redaction_policy=redaction_policy,
    )
    return scenario_dir


def generate_harness(
    *,
    out_dir: Path,
    scenarios: list[str],
    created_at: str,
    redaction_policy: str,
) -> list[Path]:
    definitions = scenario_definitions()
    if out_dir.exists() and any(out_dir.iterdir()):
        raise FileExistsError(f"output directory is not empty: {out_dir}")
    out_dir.mkdir(parents=True, exist_ok=True)
    generated = []
    for scenario_id in scenarios:
        generated.append(
            generate_scenario(
                scenario=definitions[scenario_id],
                root=out_dir,
                created_at=created_at,
                redaction_policy=redaction_policy,
            )
        )
    return generated


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out-dir", type=Path, required=True)
    parser.add_argument(
        "--scenario",
        action="append",
        choices=MVP_SCENARIOS,
        help="Scenario to generate. May be repeated. Defaults to all MVP scenarios.",
    )
    parser.add_argument("--created-at", default=utc_now())
    parser.add_argument("--redaction-policy", default="none")
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    scenarios = args.scenario or list(MVP_SCENARIOS)
    generated = generate_harness(
        out_dir=args.out_dir,
        scenarios=scenarios,
        created_at=args.created_at,
        redaction_policy=args.redaction_policy,
    )
    for path in generated:
        print(f"wrote {path}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
