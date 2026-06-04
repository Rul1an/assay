#!/usr/bin/env python3
"""Coverage-aware drift annotation (sample, derived-report only).

Reads one `assay.runner.runtime_drift.v0.2` report and attaches per-dimension
claim cells that apply the coverage ceiling at the comparator-output level.

This is the comparator-level companion to the single-archive
`coverage-aware-side-effect` sample. The single-archive sample gates claims for
one measured run; this sample gates the *reading* of a cross-runtime drift row,
so a full-overlap (`task-induced`) row is not mistaken for an exhaustive-equality
or bounded-negative effect claim.

It is a sample: it produces a placeholder annotation envelope. It does not change
the comparator, the runtime-drift schema, Runner archives, or Trust Basis. The
canonical coverage gate lives in `crates/assay-runner-schema/src/coverage.rs`;
this script mirrors its ceiling so the pattern is reviewable from a frozen
report fixture.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

ANNOTATION_SCHEMA = "assay.coverage_aware_drift.annotation.v0"
CLAIM_CELL_SCHEMA = "assay.observability.claim_class_cell.v0"
DRIFT_SCHEMA = "assay.runner.runtime_drift.v0.2"
# The gating rule named by every derived claim cell, per claim-classes-v0
# ("Derived must name the rule").
GATE_RULE = "coverage_descriptor.v0 + fidelity_verdict.v0"

# Seed coverage descriptors per effect dimension, mirroring coverage.rs. Each
# names the capture method and documented blind spots, so an exhaustive or
# bounded-negative reading is never assumed from a drift row.
SEED_DESCRIPTORS: dict[str, dict[str, Any]] = {
    "filesystem": {
        "method": "open/openat/openat2 tracepoints",
        "known_blind_spots": [
            "io_uring file operations may bypass syscall tracepoints",
            "mmap-backed writes are not path-open observations",
        ],
        "completeness": "open_syscall_only",
    },
    "network": {
        "method": "connect tracepoint",
        "known_blind_spots": [
            "QUIC/datagram peer changes after connect are not an exhaustive peer set",
            "io_uring network operations may bypass syscall tracepoints",
        ],
        "completeness": "connect_only",
    },
    "process": {
        "method": "exec tracepoint",
        "known_blind_spots": [
            "fork/clone gaps can make process-tree exhaustiveness kernel-dependent",
        ],
        "completeness": "exec_only",
    },
}

# Map each runtime-drift dimension to an effect dimension + claim basis.
# Measured dimensions get the coverage ceiling; reported dimensions do not,
# because SDK/trace events are reported, not measured kernel effects.
DIMENSION_MAP: dict[str, dict[str, str | None]] = {
    "filesystem_paths_touched": {"effect": "filesystem", "basis": "measured"},
    "kernel_file_operations": {"effect": "filesystem", "basis": "measured"},
    "network_endpoints": {"effect": "network", "basis": "measured"},
    "process_execs": {"effect": "process", "basis": "measured"},
    "sdk_tool_events": {"effect": None, "basis": "reported"},
    "tool_invocation_order": {"effect": None, "basis": "reported"},
    "mcp_tool_surface": {"effect": None, "basis": "reported"},
}


def _blind_spot_summary(descriptor: dict[str, Any]) -> str:
    spots = descriptor.get("known_blind_spots") or []
    return "; ".join(spots) if spots else "none declared"


def _supports_complete_claims(descriptor: dict[str, Any]) -> bool:
    # Mirrors coverage.rs: completeness == full AND no blind spots.
    return descriptor.get("completeness") == "full" and not descriptor.get(
        "known_blind_spots"
    )


def _row_observed_anything(row: dict[str, Any]) -> bool:
    return bool(
        row.get("only_in_a") or row.get("only_in_b") or row.get("in_both")
    )


def annotate(report: dict[str, Any]) -> dict[str, Any]:
    schema = report.get("schema")
    if schema != DRIFT_SCHEMA:
        raise ValueError(
            f"expected {DRIFT_SCHEMA} report; got {schema!r}"
        )
    rows = report.get("rows")
    if not isinstance(rows, list):
        raise ValueError("report: rows array is required")

    claim_cells: list[dict[str, Any]] = []
    blocked_claims: list[dict[str, Any]] = []
    classification_caveats: list[dict[str, Any]] = []

    for row in rows:
        dimension = row.get("dimension")
        mapping = DIMENSION_MAP.get(dimension)
        if mapping is None:
            continue
        classification = row.get("classification")
        observed = _row_observed_anything(row)

        if mapping["basis"] == "reported":
            # SDK/trace-reported surface: real for control flow, but not a
            # measured kernel effect. No coverage gate applies; the cell stays
            # reported so it is never read as a measured exhaustive set.
            if observed:
                claim_cells.append(
                    {
                        "schema": CLAIM_CELL_SCHEMA,
                        "claim_type": f"reported_{dimension}",
                        "artifact_role": "joined_artifacts",
                        "claim_strength": "partial",
                        "claim_basis": "reported",
                        "evidence_refs": ["runtime-drift-report.json"],
                        "notes": [
                            "SDK/trace-reported drift surface; not a measured "
                            "kernel effect, so no coverage ceiling is applied"
                        ],
                        "non_claims": [
                            "does_not_prove_measured_effect",
                            "does_not_prove_complete_set",
                        ],
                    }
                )
            continue

        effect = mapping["effect"]
        if effect is None:
            # Reported dimensions return above; measured ones always map to an
            # effect. Guard explicitly so the SEED_DESCRIPTORS lookup is safe.
            continue
        descriptor = SEED_DESCRIPTORS[effect]

        if observed:
            # Positive existence of the observed drift surface. The drift report
            # does not surface per-arm capture health, so positive strength is
            # capped at partial: this sample will not emit a strong measured
            # claim it cannot back from the report alone.
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"measured_{dimension}_drift",
                    "artifact_role": "joined_artifacts",
                    "claim_strength": "partial",
                    "claim_basis": "measured",
                    "evidence_refs": ["runtime-drift-report.json"],
                    "notes": [
                        "positive strength capped at partial: the drift report "
                        "does not surface per-arm observation health; consult "
                        "fidelity_verdict.v0 against the source archives to "
                        "raise this to strong"
                    ],
                    "non_claims": [
                        "does_not_prove_tool_intent",
                        "does_not_prove_complete_set",
                    ],
                }
            )

        # Exhaustive equality: the reading that in_both / task-induced means the
        # complete shared effect set. Only meaningful when something was
        # observed. Allowed (weak->partial) only when coverage supports complete
        # claims; otherwise degraded to weak. This mirrors coverage_descriptor.v0,
        # where exhaustive set is allowed when completeness=full with no blind
        # spots. Under the seed descriptors the supported branch is never taken;
        # it is kept so the example represents the full gate, not only its
        # degraded path.
        if observed and _supports_complete_claims(descriptor):
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"exhaustive_{dimension}_equality",
                    "artifact_role": "joined_artifacts",
                    "claim_strength": "partial",
                    "claim_basis": "derived",
                    "evidence_refs": ["runtime-drift-report.json"],
                    "notes": [
                        f"derived by {GATE_RULE}: completeness is full with no "
                        f"declared blind spots, so exhaustive {effect} equality is "
                        f"allowed; capped at partial because the drift report does "
                        f"not surface per-arm health"
                    ],
                    "non_claims": ["strong_only_within_cgroup_scope"],
                }
            )
        elif observed:
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"exhaustive_{dimension}_equality",
                    "artifact_role": "joined_artifacts",
                    "claim_strength": "weak",
                    "claim_basis": "derived",
                    "evidence_refs": ["runtime-drift-report.json"],
                    "notes": [
                        f"derived by {GATE_RULE}: exhaustive {effect} equality "
                        f"requires completeness=full with no blind spots; "
                        f"completeness is {descriptor['completeness']}; blind "
                        f"spots: {_blind_spot_summary(descriptor)}"
                    ],
                    "non_claims": [f"does_not_prove_complete_{effect}_set"],
                }
            )

        # Bounded negative: "no effect beyond the observed drift surface". The
        # coverage_descriptor.v0 gate allows this only when completeness=full
        # with no blind spots; this comparator-level sample additionally never
        # allows it, because the drift report does not surface per-arm capture
        # health, so a clean-capture precondition cannot be confirmed here. The
        # block reason names both gates.
        blocked_claims.append(
            {
                "claim_type": "bounded_negative_claim",
                "requested_claim": f"no_{dimension}_effect_beyond_observed",
                "decision": "blocked",
                "reason": (
                    f"{effect} absence-beyond-observed claim requires "
                    f"completeness=full with no blind spots and confirmed clean "
                    f"capture; completeness is {descriptor['completeness']}; blind "
                    f"spots: {_blind_spot_summary(descriptor)}; the drift report "
                    f"does not surface per-arm observation health"
                ),
            }
        )

        # A task-induced (full overlap) classification is descriptive surface
        # shape, not an exhaustive-equality proof. Make that explicit.
        if classification == "task-induced":
            classification_caveats.append(
                {
                    "dimension": dimension,
                    "classification": "task-induced",
                    "caveat": (
                        "full overlap of the observed surface; this is not proof "
                        f"the {effect} effect sets are exhaustively equal, because "
                        f"{effect} coverage is {descriptor['completeness']}"
                    ),
                }
            )

    return {
        "schema": ANNOTATION_SCHEMA,
        "source_report_schema": DRIFT_SCHEMA,
        "claim_cells": claim_cells,
        "blocked_claims": blocked_claims,
        "classification_caveats": classification_caveats,
    }


def render_markdown(annotation: dict[str, Any]) -> str:
    lines = ["# Coverage-Aware Drift Annotation (sample)", ""]
    lines.append("## Claims")
    for cell in annotation["claim_cells"]:
        lines.append(
            f"- {cell['claim_type']}: {cell['claim_strength']} {cell['claim_basis']}"
        )
    if annotation["blocked_claims"]:
        lines.append("")
        lines.append("## Blocked (coverage cannot support)")
        for blocked in annotation["blocked_claims"]:
            lines.append(f"- {blocked['requested_claim']}: {blocked['reason']}")
    if annotation["classification_caveats"]:
        lines.append("")
        lines.append("## Classification caveats")
        for caveat in annotation["classification_caveats"]:
            lines.append(f"- {caveat['dimension']}: {caveat['caveat']}")
    return "\n".join(lines) + "\n"


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("report", help="runtime-drift v0.2 report JSON")
    parser.add_argument("--format", choices=["json", "markdown"], default="json")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    with open(args.report, "r", encoding="utf-8") as handle:
        report = json.load(handle)
    annotation = annotate(report)
    if args.format == "markdown":
        sys.stdout.write(render_markdown(annotation))
    else:
        sys.stdout.write(json.dumps(annotation, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
