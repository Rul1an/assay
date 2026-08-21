#!/usr/bin/env python3
"""External required-CI pin for the Conformance inventory callsite.

The inventory tests already encode this contract, but they execute inside the
step they guard. Required CI therefore invokes the same rule from the later
hardening root. This file does not parse workflow YAML; it calls
assert_hard_run_command.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import assert_hard_run_command  # noqa: E402

CI_YML = REPO / ".github/workflows/ci.yml"
JOB = "scope"
STEP = "Conformance inventory"


def conformance_inventory_callsite_problems(text: str) -> list[str]:
    try:
        assert_hard_run_command(text, JOB, STEP)
    except AssertionError as exc:
        message = str(exc).strip() or "conformance inventory callsite contract failed"
        return [message]
    return []


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    path = Path(args[0]) if args else CI_YML
    problems = conformance_inventory_callsite_problems(
        path.read_text(encoding="utf-8"))
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1
    print("ok   conformance inventory callsite")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
