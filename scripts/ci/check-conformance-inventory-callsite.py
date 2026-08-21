#!/usr/bin/env python3
"""External required-CI pin for the Conformance inventory callsite.

The inventory tests already encode this contract, but they execute inside the
step they guard. Required CI therefore invokes this module from the later
hardening step and again from the final required CI job. This file does not
parse workflow YAML; it calls assert_hard_run_command and
named_step/_active_run_lines.
Canonical CI always reads the committed CI_YML. Tests inject text at the
function seam.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import (  # noqa: E402
    _active_run_lines,
    assert_hard_run_command,
    named_step,
)

CI_YML = REPO / ".github/workflows/ci.yml"
JOB = "scope"
INVENTORY_STEP = "Conformance inventory"
HARDENING_JOB = "ci"
HARDENING_STEP = "Verify CI hardening contracts"
FINALE_STEP = "Verify this gate waits on every gating job"
FINALE_CHECKER = "python3 scripts/ci/check-conformance-inventory-callsite.py"


def conformance_inventory_callsite_problems(text: str) -> list[str]:
    try:
        assert_hard_run_command(text, JOB, INVENTORY_STEP)
    except AssertionError as exc:
        message = str(exc).strip() or "conformance inventory callsite contract failed"
        return [message]
    return []


def hardening_guard_callsite_problems(text: str) -> list[str]:
    try:
        assert_hard_run_command(text, HARDENING_JOB, HARDENING_STEP)
    except AssertionError as exc:
        problems = [str(exc).strip() or "hardening hard-run contract failed"]
    else:
        problems = []
    try:
        step = named_step(text, HARDENING_JOB, FINALE_STEP)
    except AssertionError as exc:
        problems.append(str(exc).strip() or "finale CI checker step missing")
        return problems
    if FINALE_CHECKER not in _active_run_lines(step):
        problems.append("finale CI missing active hardening-step checker")
    return problems


def main() -> int:
    if len(sys.argv) != 1:
        print("FAIL: this checker reads only the committed CI_YML", file=sys.stderr)
        return 2
    text = CI_YML.read_text(encoding="utf-8")
    problems = (
        conformance_inventory_callsite_problems(text)
        + hardening_guard_callsite_problems(text)
    )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1
    print("ok   conformance inventory callsite")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
