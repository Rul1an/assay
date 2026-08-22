#!/usr/bin/env python3
"""External required-CI pin for the Conformance inventory callsite.

The inventory tests already encode this contract, but they execute inside the
step they guard. Required CI therefore invokes this module from the later
hardening step and again from the final required CI job. The required
aggregator scheduling condition (`jobs.ci.if: always()`) is checked from the
already-required host-capability-check workflow; a checker inside `jobs.ci`
cannot prove that its own parent job was scheduled. This file does not
parse workflow YAML; it calls assert_hard_run_command / assert_hard_run_successor
/ assert_required_aggregator_schedule / assert_host_schedule_invocation
and the shared indentation/direct-key helpers.
Canonical CI always reads the committed CI_YML. Tests inject text at the
function seam.

Hardening and finale own each other under a single-mutation contract: changing
only one root must turn the remaining caller red. The host job is the
scheduling root only because it has no if/needs; the CI checker pins that
absence. Simultaneous mutation of both required roots (jobs.ci.if and the
host check job if/needs) is outside repo-local enforcement. This is not
self-enforcement against coordinated workflow edits.
"""

from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import (  # noqa: E402
    assert_hard_run_command,
    assert_hard_run_successor,
    assert_host_schedule_invocation,
    assert_required_aggregator_schedule,
)

CI_YML = REPO / ".github/workflows/ci.yml"
HOST_YML = REPO / ".github/workflows/host-capability-check.yml"
JOB = "scope"
INVENTORY_STEP = "Conformance inventory"
HARDENING_JOB = "ci"
HARDENING_STEP = "Verify CI hardening contracts"
FINALE_STEP = "Verify this gate waits on every gating job"
SCHEDULE_FLAG = "--required-aggregator-schedule"


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
        assert_hard_run_successor(
            text, HARDENING_JOB, HARDENING_STEP, FINALE_STEP)
    except AssertionError as exc:
        problems.append(str(exc).strip() or "finale hard-run successor failed")
    return problems


def required_aggregator_schedule_problems(text: str) -> list[str]:
    try:
        assert_required_aggregator_schedule(text)
    except AssertionError as exc:
        message = str(exc).strip() or "required aggregator schedule failed"
        return [message]
    return []


def host_schedule_callsite_problems(text: str) -> list[str]:
    try:
        assert_host_schedule_invocation(text)
    except AssertionError as exc:
        message = str(exc).strip() or "host scheduling callsite failed"
        return [message]
    return []


def main() -> int:
    args = sys.argv[1:]
    if args and args != [SCHEDULE_FLAG]:
        print("FAIL: this checker reads only the committed CI_YML", file=sys.stderr)
        return 2
    text = CI_YML.read_text(encoding="utf-8")
    if args == [SCHEDULE_FLAG]:
        problems = required_aggregator_schedule_problems(text)
    else:
        problems = (
            conformance_inventory_callsite_problems(text)
            + hardening_guard_callsite_problems(text)
            + required_aggregator_schedule_problems(text)
            + host_schedule_callsite_problems(HOST_YML.read_text(encoding="utf-8"))
        )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1
    print("ok   conformance inventory callsite")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
