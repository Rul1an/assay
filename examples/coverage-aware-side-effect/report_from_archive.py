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


def _blind_spot_summary(descriptor: dict[str, Any]) -> str:
    spots = descriptor.get("known_blind_spots") or []
    return "; ".join(spots) if spots else "none declared"


def _supports_complete_claims(descriptor: dict[str, Any]) -> bool:
    # Mirrors coverage.rs: completeness == full AND no blind spots.
    return descriptor.get("completeness") == "full" and not descriptor.get(
        "known_blind_spots"
    )


def _capture_is_clean(health: dict[str, Any]) -> bool:
    return (
        health.get("kernel_layer") == "complete"
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
                "claim_type": f"measured_{dimension}_effect",
                "artifact_role": "measured_run_archive",
                "claim_strength": "strong" if capture_clean else "partial",
                "claim_basis": "measured",
                "evidence_refs": ["capability-surface.json", "observation-health.json"],
                "notes": [],
                "non_claims": [
                    "does_not_prove_intent",
                    "strong_only_within_cgroup_scope",
                ],
            }
        )

        # Exhaustive set: "these are all the X". Allowed only when the descriptor
        # supports complete claims; otherwise degraded to weak with the reason.
        if not _supports_complete_claims(descriptor):
            claim_cells.append(
                {
                    "schema": CLAIM_CELL_SCHEMA,
                    "claim_type": f"exhaustive_{dimension}_set",
                    "artifact_role": "measured_run_archive",
                    "claim_strength": "weak",
                    "claim_basis": "measured",
                    "evidence_refs": ["capability-surface.json", "observation-health.json"],
                    "notes": [
                        f"exhaustive {dimension} set requires completeness=full with no "
                        f"blind spots; completeness is {descriptor['completeness']}; "
                        f"blind spots: {_blind_spot_summary(descriptor)}"
                    ],
                    "non_claims": [f"does_not_prove_complete_{dimension}_set"],
                }
            )

        # Bounded negative: "X did not happen". Blocked unless coverage is
        # complete with no blind spots AND capture was clean.
        if not _supports_complete_claims(descriptor) or not capture_clean:
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
