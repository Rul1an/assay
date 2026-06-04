#!/usr/bin/env python3
"""Coverage-claims gate (sample consumer, enforcement showcase).

Reads a coverage annotation sidecar (`assay.coverage_aware_drift.annotation.v0`,
as produced by the cross-runtime drift comparator's --coverage-annotation-out)
and a set of asserted coverage claims, then mechanically decides pass/fail:

    permitted  -> exit 0
    any blocked -> exit 1

This is the consumer side of the honesty layer: it shows that a downstream gate
(e.g. a CI step) can refuse to assert a coverage claim the annotation does not
permit, instead of silently treating absence or reported signal as evidence.

It is a sample: it does not change any Runner/contract surface. The canonical
gate semantics live in crates/assay-runner-schema/src/coverage.rs; this checker
mirrors the same claim-kind rules over a frozen annotation document.
"""

from __future__ import annotations

import argparse
import json
import sys
from typing import Any

ANNOTATION_SCHEMA = "assay.coverage_aware_drift.annotation.v0"
ASSERTABLE_CLAIM_TYPES = ("positive", "exhaustive", "bounded_negative")

# Which drift dimensions are measured (effect-bearing) vs reported. A
# bounded-negative claim is only evaluable for a measured dimension; on a
# reported/unknown dimension it is not evaluable and therefore not permitted.
MEASURED_DIMENSIONS = {
    "filesystem_paths_touched",
    "kernel_file_operations",
    "network_endpoints",
    "process_execs",
}


def _cells(annotation: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {c.get("claim_type"): c for c in annotation.get("claim_cells", [])}


def evaluate_claim(
    annotation: dict[str, Any], claim_type: str, dimension: str
) -> tuple[bool, str]:
    """Return (permitted, detail) for one asserted claim against an annotation.

    - positive:DIM permitted iff a measured_{DIM}_drift cell exists with strength
      strong or partial (absent / missing -> not permitted).
    - exhaustive:DIM permitted iff exhaustive_{DIM}_equality is allowed
      (strength partial); a degraded weak cell or a missing cell is not.
    - bounded_negative:DIM permitted iff DIM is a measured dimension AND it is
      not present in blocked_claims. Reported/unknown dimensions are not
      evaluable -> not permitted.
    """
    cells = _cells(annotation)
    if claim_type == "positive":
        cell = cells.get(f"measured_{dimension}_drift")
        if cell is None:
            return False, f"no measured_{dimension}_drift cell (nothing observed)"
        strength = cell.get("claim_strength")
        if strength in ("strong", "partial"):
            return True, f"measured positive is {strength}"
        return False, f"measured positive is {strength}"
    if claim_type == "exhaustive":
        cell = cells.get(f"exhaustive_{dimension}_equality")
        if cell is None:
            return False, f"no exhaustive_{dimension}_equality cell"
        strength = cell.get("claim_strength")
        if strength == "partial":
            return True, "exhaustive equality allowed (partial)"
        return False, f"exhaustive equality is {strength} (degraded by coverage)"
    if claim_type == "bounded_negative":
        if dimension not in MEASURED_DIMENSIONS:
            return (
                False,
                f"bounded-negative not evaluable for non-measured dimension "
                f"{dimension!r}",
            )
        blocked = {
            b.get("requested_claim") for b in annotation.get("blocked_claims", [])
        }
        if f"no_{dimension}_effect_beyond_observed" in blocked:
            return False, "bounded-negative blocked by coverage descriptor"
        return True, "bounded-negative not blocked"
    return False, f"unknown claim type {claim_type!r}"


def parse_claim_spec(spec: str) -> tuple[str, str]:
    raw = str(spec)
    if ":" not in raw:
        raise ValueError(f"claim must be TYPE:DIMENSION, got {raw!r}")
    claim_type, _, dimension = raw.partition(":")
    claim_type = claim_type.strip()
    dimension = dimension.strip()
    if claim_type not in ASSERTABLE_CLAIM_TYPES:
        raise ValueError(
            f"claim TYPE must be one of {ASSERTABLE_CLAIM_TYPES}; got {claim_type!r}"
        )
    if not dimension:
        raise ValueError(f"claim must have a non-empty dimension, got {raw!r}")
    return claim_type, dimension


def gate(annotation: dict[str, Any], specs: list[str]) -> dict[str, Any]:
    """Evaluate all asserted claims; return a deterministic report."""
    if annotation.get("schema") != ANNOTATION_SCHEMA:
        raise ValueError(
            f"expected {ANNOTATION_SCHEMA} annotation; got {annotation.get('schema')!r}"
        )
    results = []
    for spec in specs:
        claim_type, dimension = parse_claim_spec(spec)
        permitted, detail = evaluate_claim(annotation, claim_type, dimension)
        results.append(
            {
                "claim": f"{claim_type}:{dimension}",
                "permitted": permitted,
                "detail": detail,
            }
        )
    passed = all(r["permitted"] for r in results)
    return {"passed": passed, "results": results}


def render_text(report: dict[str, Any]) -> str:
    lines = []
    for r in report["results"]:
        mark = "PERMIT" if r["permitted"] else "BLOCK "
        lines.append(f"[{mark}] {r['claim']}: {r['detail']}")
    lines.append("PASS" if report["passed"] else "FAIL")
    return "\n".join(lines) + "\n"


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "annotation",
        help="coverage annotation JSON (assay.coverage_aware_drift.annotation.v0)",
    )
    parser.add_argument(
        "--assert-claim",
        action="append",
        default=[],
        metavar="TYPE:DIMENSION",
        help="A coverage claim to assert (positive|exhaustive|bounded_negative:DIM). Repeatable.",
    )
    parser.add_argument(
        "--policy",
        help="Optional JSON file with an array of TYPE:DIMENSION claim strings.",
    )
    parser.add_argument("--format", choices=["text", "json"], default="text")
    return parser.parse_args()


def _run(args: argparse.Namespace) -> int:
    with open(args.annotation, "r", encoding="utf-8") as handle:
        annotation = json.load(handle)
    specs = list(args.assert_claim)
    if args.policy:
        with open(args.policy, "r", encoding="utf-8") as handle:
            policy = json.load(handle)
        if not isinstance(policy, list):
            print("--policy file must contain a JSON array of claim strings", file=sys.stderr)
            return 2
        specs.extend(str(item) for item in policy)
    if not specs:
        print("no claims asserted (use --assert-claim or --policy)", file=sys.stderr)
        return 2
    try:
        report = gate(annotation, specs)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    if args.format == "json":
        sys.stdout.write(json.dumps(report, indent=2, sort_keys=True) + "\n")
    else:
        sys.stdout.write(render_text(report))
    return 0 if report["passed"] else 1


if __name__ == "__main__":
    raise SystemExit(_run(_parse_args()))
