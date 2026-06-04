#!/usr/bin/env python3
"""Coverage fleet summary (sample aggregator, governance showcase).

Reads many coverage annotation sidecars
(`assay.coverage_aware_drift.annotation.v0`, as produced by the cross-runtime
drift comparator's --coverage-annotation-out) and folds them into one
fleet-level honesty summary: for each measured dimension, how many runs report
each positive strength, how many allow an exhaustive claim, and how many block
the bounded-negative claim — plus the fleet "floor" (the weakest strength seen
across the set).

The point of the showcase: the same per-run honesty classification scales to a
whole set of runs using only local inputs and with no contract change — it is
just a deterministic fold over annotation documents on disk. A team can answer
"across these N runs, which coverage claims are actually supportable
everywhere?" from local files alone.

It is a sample: it changes no Runner/contract surface. It consumes only the
public annotation sidecar shape and emits an example-scoped v0 summary.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from typing import Any

ANNOTATION_SCHEMA = "assay.coverage_aware_drift.annotation.v0"
SUMMARY_SCHEMA = "assay.coverage_fleet_summary.v0"

MEASURED_DIMENSIONS = (
    "filesystem_paths_touched",
    "kernel_file_operations",
    "network_endpoints",
    "process_execs",
)

# Positive strength ordering, weakest first — used to compute the fleet floor.
_STRENGTH_ORDER = ("absent", "weak", "partial", "strong")


def _weaker(a: str, b: str) -> str:
    """Return the weaker of two strengths by _STRENGTH_ORDER (unknowns are weakest)."""
    ia = _STRENGTH_ORDER.index(a) if a in _STRENGTH_ORDER else -1
    ib = _STRENGTH_ORDER.index(b) if b in _STRENGTH_ORDER else -1
    return a if ia <= ib else b


def _empty_dimension() -> dict[str, Any]:
    return {
        "measured_positive": {"strong": 0, "partial": 0, "weak": 0, "absent": 0, "missing": 0},
        "exhaustive_equality": {"partial": 0, "weak": 0, "absent": 0, "missing": 0},
        "bounded_negative_blocked": 0,
        "runs_observed": 0,
        "fleet_positive_floor": "missing",
    }


def _cells(annotation: dict[str, Any]) -> dict[str, dict[str, Any]]:
    return {c.get("claim_type"): c for c in annotation.get("claim_cells", [])}


def _blocked(annotation: dict[str, Any]) -> set[str]:
    return {b.get("requested_claim") for b in annotation.get("blocked_claims", [])}


def fold(annotations: list[dict[str, Any]]) -> dict[str, Any]:
    """Fold a list of annotation documents into a fleet summary."""
    dims: dict[str, dict[str, Any]] = {d: _empty_dimension() for d in MEASURED_DIMENSIONS}
    for annotation in annotations:
        if annotation.get("schema") != ANNOTATION_SCHEMA:
            raise ValueError(
                f"expected {ANNOTATION_SCHEMA} annotation; got {annotation.get('schema')!r}"
            )
        cells = _cells(annotation)
        blocked = _blocked(annotation)
        for dim in MEASURED_DIMENSIONS:
            entry = dims[dim]
            pos = cells.get(f"measured_{dim}_drift")
            if pos is None:
                entry["measured_positive"]["missing"] += 1
            else:
                strength = pos.get("claim_strength", "missing")
                if strength in entry["measured_positive"] and strength != "missing":
                    entry["measured_positive"][strength] += 1
                    entry["runs_observed"] += 1
                    cur = entry["fleet_positive_floor"]
                    entry["fleet_positive_floor"] = (
                        strength if cur == "missing" else _weaker(cur, strength)
                    )
                else:
                    entry["measured_positive"]["missing"] += 1

            exh = cells.get(f"exhaustive_{dim}_equality")
            if exh is None:
                entry["exhaustive_equality"]["missing"] += 1
            else:
                strength = exh.get("claim_strength", "missing")
                if strength in entry["exhaustive_equality"]:
                    entry["exhaustive_equality"][strength] += 1
                else:
                    entry["exhaustive_equality"]["missing"] += 1

            if f"no_{dim}_effect_beyond_observed" in blocked:
                entry["bounded_negative_blocked"] += 1

    # A dimension never observed across the fleet has no floor.
    for entry in dims.values():
        if entry["runs_observed"] == 0:
            entry["fleet_positive_floor"] = "missing"

    return {
        "schema": SUMMARY_SCHEMA,
        "run_count": len(annotations),
        "dimensions": dims,
    }


def render_text(summary: dict[str, Any]) -> str:
    lines = [f"fleet coverage summary over {summary['run_count']} run(s)", ""]
    for dim in sorted(summary["dimensions"]):
        entry = summary["dimensions"][dim]
        pos = entry["measured_positive"]
        pos_str = ", ".join(
            f"{k}={pos[k]}" for k in ("strong", "partial", "weak", "absent", "missing")
        )
        lines.append(f"{dim}:")
        lines.append(f"  positive floor: {entry['fleet_positive_floor']}  ({pos_str})")
        lines.append(f"  bounded-negative blocked in {entry['bounded_negative_blocked']} run(s)")
    return "\n".join(lines) + "\n"


def _read_annotations(paths: list[str]) -> list[dict[str, Any]]:
    annotations = []
    for path in paths:
        with open(path, "r", encoding="utf-8") as handle:
            annotations.append(json.load(handle))
    return annotations


def _collect_paths(args: argparse.Namespace) -> list[str]:
    paths = list(args.annotation)
    if args.dir:
        for name in sorted(os.listdir(args.dir)):
            if name.endswith(".json"):
                paths.append(os.path.join(args.dir, name))
    return paths


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "annotation",
        nargs="*",
        help="annotation JSON file(s) (assay.coverage_aware_drift.annotation.v0)",
    )
    parser.add_argument(
        "--dir",
        help="directory of annotation .json files (sorted, non-recursive)",
    )
    parser.add_argument("--format", choices=["text", "json"], default="text")
    return parser.parse_args()


def _run(args: argparse.Namespace) -> int:
    paths = _collect_paths(args)
    if not paths:
        print("no annotations supplied (pass files or --dir)", file=sys.stderr)
        return 2
    try:
        annotations = _read_annotations(paths)
        summary = fold(annotations)
    except ValueError as exc:
        print(f"error: {exc}", file=sys.stderr)
        return 2
    if args.format == "json":
        sys.stdout.write(json.dumps(summary, indent=2, sort_keys=True) + "\n")
    else:
        sys.stdout.write(render_text(summary))
    return 0


if __name__ == "__main__":
    raise SystemExit(_run(_parse_args()))
