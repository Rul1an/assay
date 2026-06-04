#!/usr/bin/env python3
"""Coverage-aware side-effect report generator (sample, derived-report only).

Reads the observation_health and capability_surface of a Runner archive (as a
single JSON fixture for this sample) and emits per-dimension claim cells plus
blocked claims, applying the same claim-kind gate as the shipped
`assay.runner.coverage_descriptor.v0` helper.

This is a sample: it produces placeholder report envelopes. It does not register
a new archive member, does not change Runner schemas, and does not promote
anything into Trust Basis. The canonical gate logic lives in
`crates/assay-runner-schema/src/coverage.rs`; this script mirrors it so the
pattern is reviewable from a frozen fixture.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

REPORT_SCHEMA = "assay.coverage_aware_side_effect.report.v0"
CLAIM_CELL_SCHEMA = "assay.observability.claim_class_cell.v0"
# The gating rule named by every derived claim cell, per claim-classes-v0
# ("Derived must name the rule").
GATE_RULE = "coverage_descriptor.v0 + fidelity_verdict.v0"

# Seed coverage descriptors per effect dimension, mirroring the shipped
# coverage.rs seed constructors. Each names the capture method and its
# documented blind spots, so completeness is never assumed.
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

# Which capability_surface field carries the observed set for each dimension.
SURFACE_FIELD = {
    "filesystem": "filesystem_paths",
    "network": "network_endpoints",
    "process": "process_execs",
}

# Canonical positive-effect claim_type per dimension, matching the documented
# vocabulary in docs/reference/observability/claim-classes-v0.md. Note the
# process dimension uses `process_execution_effect`, not `measured_process_*`.
POSITIVE_CLAIM_TYPE = {
    "filesystem": "measured_filesystem_effect",
    "network": "measured_network_effect",
    "process": "process_execution_effect",
}


def _blind_spot_summary(descriptor: dict[str, Any]) -> str:
    spots = descriptor.get("known_blind_spots") or []
    return "; ".join(spots) if spots else "none declared"


def _supports_complete_claims(descriptor: dict[str, Any]) -> bool:
    # Mirrors coverage.rs: completeness == full AND no blind spots.
    return descriptor.get("completeness") == "full" and not descriptor.get(
        "known_blind_spots"
    )


def _capture_is_clean(health: dict[str, Any]) -> bool:
    # Non-Linux records have no measured kernel-effect surface; the canonical
    # fidelity verdict treats them as not_applicable, never clean, so they must
    # not upgrade measured claims to strong.
    return (
        health.get("platform") == "linux"
        and health.get("kernel_layer") == "complete"
        and int(health.get("ringbuf_drops", 1)) == 0
        and health.get("cgroup_correlation") == "clean"
    )


def build_report(archive: dict[str, Any]) -> dict[str, Any]:
    health = archive.get("observation_health")
    surface = archive.get("capability_surface")
    if not isinstance(health, dict):
        raise ValueError("archive: observation_health object is required")
    if not isinstance(surface, dict):
        raise ValueError("archive: capability_surface object is required")

    run_id = health.get("run_id") or surface.get("run_id") or "unknown"
    capture_clean = _capture_is_clean(health)
    claim_cells: list[dict[str, Any]] = []
    blocked_claims: list[dict[str, Any]] = []

    for dimension, descriptor in SEED_DESCRIPTORS.items():
        observed = surface.get(SURFACE_FIELD[dimension]) or []
        if not observed:
            continue

        # Positive existence: an observed effect happened. Capture health gates
        # strength (clipped capture downgrades positive measured claims), but the
        # descriptor's blind spots do not weaken a positive observation.
        claim_cells.append(
            {
                "schema": CLAIM_CELL_SCHEMA,
                "claim_type": POSITIVE_CLAIM_TYPE[dimension],
                "artifact_role": "measured_run_archive",
                "claim_strength": "strong" if capture_clean else "partial",
                "claim_basis": "measured",
                "evidence_refs": ["capability-surface.json", "observation-health.json"],
                "notes": [],
                "non_claims": [
                    "does_not_prove_tool_intent",
                    "strong_only_within_cgroup_scope",
                ],
            }
        )

        # Exhaustive set: "these are all the X". Allowed (strong) only when the
        # descriptor supports complete claims; otherwise degraded to weak with
        # the reason. This mirrors coverage_descriptor.v0, where exhaustive set
        # is allowed when completeness=full with no blind spots. Under the seed
        # descriptors the supported branch is never taken; it is kept so the
        # example represents the full gate, not only its degraded path.
        if _supports_complete_claims(descriptor):
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"exhaustive_{dimension}_set",
                    "artifact_role": "measured_run_archive",
                    "claim_strength": "strong" if capture_clean else "partial",
                    "claim_basis": "derived",
                    "evidence_refs": ["capability-surface.json", "observation-health.json"],
                    "notes": [
                        f"derived by {GATE_RULE}: completeness is full with no "
                        f"declared blind spots, so the exhaustive {dimension} set "
                        f"is allowed"
                    ],
                    "non_claims": ["strong_only_within_cgroup_scope"],
                }
            )
        else:
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"exhaustive_{dimension}_set",
                    "artifact_role": "measured_run_archive",
                    "claim_strength": "weak",
                    "claim_basis": "derived",
                    "evidence_refs": ["capability-surface.json", "observation-health.json"],
                    "notes": [
                        f"derived by {GATE_RULE}: exhaustive {dimension} set requires "
                        f"completeness=full with no blind spots; completeness is "
                        f"{descriptor['completeness']}; blind spots: "
                        f"{_blind_spot_summary(descriptor)}"
                    ],
                    "non_claims": [f"does_not_prove_complete_{dimension}_set"],
                }
            )

        # Bounded negative: "X did not happen". Allowed only when coverage is
        # complete with no blind spots AND capture was clean; otherwise blocked.
        # As with the exhaustive set, the seed descriptors never satisfy the
        # allowed branch; it is kept so the gate's allowed outcome is represented.
        if _supports_complete_claims(descriptor) and capture_clean:
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": "bounded_negative_claim",
                    "artifact_role": "measured_run_archive",
                    "claim_strength": "strong",
                    "claim_basis": "derived",
                    "evidence_refs": ["capability-surface.json", "observation-health.json"],
                    "notes": [
                        f"{dimension}: derived by {GATE_RULE}: completeness is full "
                        f"with no declared blind spots and capture is clean, so the "
                        f"bounded-negative claim is allowed"
                    ],
                    "non_claims": ["strong_only_within_cgroup_scope"],
                }
            )
        else:
            reason = (
                f"{dimension} absence claim requires completeness=full with no blind "
                f"spots and clean capture; completeness is {descriptor['completeness']}; "
                f"blind spots: {_blind_spot_summary(descriptor)}; "
                f"capture_clean={str(capture_clean).lower()}"
            )
            blocked_claims.append(
                {
                    "claim_type": "bounded_negative_claim",
                    "requested_claim": f"no_unexpected_{dimension}_effect",
                    "decision": "blocked",
                    "reason": reason,
                }
            )

    return {
        "schema": REPORT_SCHEMA,
        "run_id": run_id,
        "observation_health_ref": "observation-health.json",
        "claim_cells": claim_cells,
        "blocked_claims": blocked_claims,
    }


def render_markdown(report: dict[str, Any]) -> str:
    lines = ["# Coverage-Aware Side-Effect Report (sample)", ""]
    lines.append(f"run id: `{report['run_id']}`")
    lines.append("")
    lines.append("## Claims")
    for cell in report["claim_cells"]:
        lines.append(
            f"- {cell['claim_type']}: {cell['claim_strength']} {cell['claim_basis']}"
        )
    if report["blocked_claims"]:
        lines.append("")
        lines.append("## Blocked (measurement cannot support)")
        for blocked in report["blocked_claims"]:
            lines.append(f"- {blocked['requested_claim']}: {blocked['reason']}")
    return "\n".join(lines) + "\n"


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("archive", help="archive fixture JSON (observation_health + capability_surface)")
    parser.add_argument("--format", choices=["json", "markdown"], default="json")
    return parser.parse_args()


def main() -> int:
    args = _parse_args()
    with open(args.archive, "r", encoding="utf-8") as handle:
        archive = json.load(handle)
    report = build_report(archive)
    if args.format == "markdown":
        sys.stdout.write(render_markdown(report))
    else:
        sys.stdout.write(json.dumps(report, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
