#!/usr/bin/env python3
"""Validate the focused MCP upstream-reference result set.

The conformance CLI's exit code is useful but not sufficient for this lane:
an inapplicable scenario may be skipped successfully, and a future scenario
rename could leave no evidence for the behavior this lane names. This script
therefore requires one result file per selected scenario and the exact
SEP-2322 check IDs that carry those behaviors.
"""

from __future__ import annotations

import argparse
import json
from pathlib import Path
from typing import Any


EXPECTED: dict[str, frozenset[str]] = {
    "sep-2322-client-request-state": frozenset(
        {
            "sep-2322-client-request-state-echoed",
            "sep-2322-client-jsonrpc-id-different",
            "sep-2322-client-no-state-omitted",
            "sep-2322-client-parallel-isolation",
            "sep-2322-default-result-type-complete",
        }
    ),
    "input-required-result-result-type": frozenset(
        {
            "sep-2322-result-type-included",
            "wire-schema-valid",
        }
    ),
    "input-required-result-request-state": frozenset(
        {
            "sep-2322-request-state-incomplete",
            "sep-2322-request-state-complete",
            "wire-schema-valid",
        }
    ),
}

ALLOWED_STATUSES = frozenset({"SUCCESS", "INFO"})


class ValidationError(ValueError):
    """The result set does not prove the focused reference run."""


def _scenario_for(path: Path) -> str | None:
    parent = path.parent.name
    for scenario in EXPECTED:
        if parent.startswith(f"{scenario}-") or parent.startswith(
            f"server-{scenario}-"
        ):
            return scenario
    return None


def _load_checks(path: Path) -> list[dict[str, Any]]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as error:
        raise ValidationError(f"{path}: unreadable checks JSON: {error}") from error

    if not isinstance(document, list):
        raise ValidationError(f"{path}: checks document must be an array")

    checks: list[dict[str, Any]] = []
    for index, item in enumerate(document):
        if not isinstance(item, dict):
            raise ValidationError(f"{path}: check {index} must be an object")
        check_id = item.get("id")
        status = item.get("status")
        if not isinstance(check_id, str) or not check_id:
            raise ValidationError(f"{path}: check {index} has no string id")
        if status not in ALLOWED_STATUSES:
            raise ValidationError(
                f"{path}: check {check_id!r} has non-success status {status!r}"
            )
        checks.append(item)
    return checks


def validate(results: Path) -> dict[str, int]:
    if not results.is_dir():
        raise ValidationError(f"results directory does not exist: {results}")

    found: dict[str, list[Path]] = {scenario: [] for scenario in EXPECTED}
    for checks_path in sorted(results.rglob("checks.json")):
        scenario = _scenario_for(checks_path)
        if scenario is None:
            raise ValidationError(
                f"unexpected checks file outside the focused scenarios: {checks_path}"
            )
        found[scenario].append(checks_path)

    summary: dict[str, int] = {}
    for scenario, expected_ids in EXPECTED.items():
        paths = found[scenario]
        if len(paths) != 1:
            raise ValidationError(
                f"{scenario}: expected exactly one checks.json, found {len(paths)}"
            )

        checks = _load_checks(paths[0])
        for expected_id in expected_ids:
            matching = [item for item in checks if item["id"] == expected_id]
            if len(matching) != 1:
                raise ValidationError(
                    f"{scenario}: expected check {expected_id!r} exactly once, "
                    f"found {len(matching)}"
                )
            if matching[0]["status"] != "SUCCESS":
                raise ValidationError(
                    f"{scenario}: expected check {expected_id!r} must be SUCCESS, "
                    f"found {matching[0]['status']!r}"
                )
        summary[scenario] = len(checks)

    return summary


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("results", type=Path)
    args = parser.parse_args()

    try:
        summary = validate(args.results)
    except ValidationError as error:
        print(f"FAIL: {error}")
        return 1

    print("MCP upstream reference checks verified:")
    for scenario, count in summary.items():
        print(f"- {scenario}: {count} check records")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
