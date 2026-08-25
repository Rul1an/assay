#!/usr/bin/env python3
"""Fail-closed bind of OSV scanner JSON vulnerability count to reporter SARIF results.

One rule, one function: refuse when the JSON `vulnerabilities` walk and the SARIF
`runs[].results` length disagree, or when either artifact is missing/malformed.
A second named function owns the scanner outcome gate: non-success plus a zero
JSON count is an execution failure, not a clean scan.

The scheduled workflow invokes this script; it does not reimplement the walk.
OSV_OUTCOME must be wired from steps.osv-scan.outcome.

Does not claim live scanner/reporter schema compatibility beyond these two counts.
"""

from __future__ import annotations

import json
import os
import sys
from pathlib import Path
from typing import Any


def vulnerability_count(data: Any) -> int:
    count = 0

    def walk(value: Any) -> None:
        nonlocal count
        if isinstance(value, dict):
            for key, child in value.items():
                if key == "vulnerabilities" and isinstance(child, list):
                    count += len(child)
                walk(child)
        elif isinstance(value, list):
            for item in value:
                walk(item)

    walk(data)
    return count


def sarif_result_count(data: Any) -> int:
    if not isinstance(data, dict):
        raise ValueError("OSV SARIF root is not an object")
    runs = data.get("runs")
    if not isinstance(runs, list):
        raise ValueError("OSV SARIF is missing a runs list")
    count = 0
    for run in runs:
        if not isinstance(run, dict):
            raise ValueError("OSV SARIF runs[] entry is not an object")
        results = run.get("results")
        if not isinstance(results, list):
            raise ValueError("OSV SARIF runs[].results is not a list")
        count += len(results)
    return count


def read_osv_outcome() -> str:
    outcome = os.environ.get("OSV_OUTCOME")
    if outcome is None or outcome == "":
        print(
            "::error::OSV_OUTCOME is unset or empty; refusing (wiring hole)",
            file=sys.stderr,
        )
        raise SystemExit(1)
    return outcome


def refuse_if_scanner_execution_failed(outcome: str, json_count: int) -> None:
    """Non-success with zero JSON vulns is a scanner crash, not a clean scan."""
    if outcome != "success" and json_count == 0:
        print(
            "::error::OSV scanner exited non-zero without vulnerability results; treating this as scanner execution failure",
            file=sys.stderr,
        )
        raise SystemExit(1)


def refuse_if_counts_differ(json_count: int, sarif_count: int) -> None:
    """The compare/exit. Mutating this is the false-green hole."""
    if json_count != sarif_count:
        print(
            "::error::OSV reporter SARIF result count "
            f"{sarif_count} does not match scanner JSON "
            f"vulnerability count {json_count}",
            file=sys.stderr,
        )
        raise SystemExit(1)


def bind(json_path: Path, sarif_path: Path) -> None:
    outcome = read_osv_outcome()
    if not json_path.is_file() or json_path.stat().st_size == 0:
        print(f"::error::OSV JSON result missing: {json_path}", file=sys.stderr)
        raise SystemExit(1)
    if not sarif_path.is_file() or sarif_path.stat().st_size == 0:
        print(f"::error::OSV SARIF result missing: {sarif_path}", file=sys.stderr)
        raise SystemExit(1)
    try:
        json_data = json.loads(json_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"::error::OSV JSON is malformed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    try:
        sarif_data = json.loads(sarif_path.read_text(encoding="utf-8"))
    except json.JSONDecodeError as exc:
        print(f"::error::OSV SARIF is malformed: {exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    try:
        json_count = vulnerability_count(json_data)
        sarif_count = sarif_result_count(sarif_data)
    except ValueError as exc:
        print(f"::error::{exc}", file=sys.stderr)
        raise SystemExit(1) from exc
    print(f"osv-json-vulnerabilities={json_count}")
    print(f"osv-sarif-results={sarif_count}")
    refuse_if_scanner_execution_failed(outcome, json_count)
    refuse_if_counts_differ(json_count, sarif_count)


def main() -> int:
    if len(sys.argv) != 1:
        print("usage: bind-osv-json-sarif-counts.py", file=sys.stderr)
        return 2
    try:
        bind(Path("osv-results.json"), Path("osv-results.sarif"))
    except SystemExit as exc:
        code = exc.code
        return int(code) if isinstance(code, int) else 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
