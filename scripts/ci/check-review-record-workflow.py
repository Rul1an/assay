#!/usr/bin/env python3
"""Structural contract for the non-required exact-head review-record workflow."""
from __future__ import annotations

import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import (  # noqa: E402
    _active_run_lines,
    _direct_mapping,
    _ordered_job_steps,
    named_job,
)

WORKFLOW = REPO / ".github/workflows/review-record-check.yml"
CI_WORKFLOW = REPO / ".github/workflows/ci.yml"
HOST_WORKFLOW = REPO / ".github/workflows/host-capability-check.yml"
CHECKOUT_ACTION = "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09"
ROOT_STEP = "Verify review-record workflow contract"
ROOT_SCRIPT = (
    "set -euo pipefail",
    "python3 scripts/ci/check-review-record-workflow.py",
)


def _mapping_block(text: str, key: str, indent: int) -> str:
    prefix = " " * indent + key + ":"
    lines = text.splitlines(keepends=True)
    starts = [i for i, line in enumerate(lines) if line.rstrip("\r\n") == prefix]
    if len(starts) != 1:
        raise AssertionError(
            f"expected one {key!r} mapping at indent {indent}, found {len(starts)}")
    start = starts[0]
    end = start + 1
    while end < len(lines):
        raw = lines[end]
        if raw.strip() and len(raw) - len(raw.lstrip(" ")) <= indent:
            break
        end += 1
    return "".join(lines[start:end])


def _top_level_keys(text: str) -> frozenset[str]:
    keys: list[str] = []
    for raw in text.splitlines():
        if not raw or raw.startswith("#") or raw.startswith(" "):
            continue
        if ":" not in raw:
            raise AssertionError(f"unrecognized top-level workflow line: {raw!r}")
        keys.append(raw.split(":", 1)[0])
    if len(keys) != len(set(keys)):
        raise AssertionError("duplicate top-level workflow key")
    return frozenset(keys)


def _step_name(step: str) -> str:
    entries = _direct_mapping(step)
    return _value(entries.get("name", ""))


def _value(raw: str) -> str:
    """Return the exact scalar spelling, excluding a supported inline comment."""
    return raw.split(" #", 1)[0].strip()


def _exact_run_step(step: str, name: str, script: tuple[str, ...]) -> None:
    entries = _direct_mapping(step)
    if frozenset(entries) != frozenset({"name", "shell", "run"}):
        raise AssertionError(f"{name} step keys drifted: {sorted(entries)}")
    if _value(entries["name"]) != name or _value(entries["shell"]) != "bash":
        raise AssertionError(f"{name} name/shell drifted")
    if tuple(_active_run_lines(step)) != script:
        raise AssertionError(f"{name} script drifted")


def _workflow_problems(text: str) -> list[str]:
    try:
        if _top_level_keys(text) != frozenset({"name", "on", "permissions", "jobs"}):
            raise AssertionError("workflow top-level keys drifted")

        trigger = _mapping_block(text, "on", 0)
        trigger_entries = _direct_mapping(trigger)
        if frozenset(trigger_entries) != frozenset({"pull_request"}):
            raise AssertionError("only pull_request may trigger this workflow")
        pull = _mapping_block(trigger, "pull_request", 2)
        pull_entries = _direct_mapping(pull)
        expected_types = "[opened, reopened, synchronize, ready_for_review]"
        if frozenset(pull_entries) != frozenset({"types"}) or _value(
            pull_entries.get("types", "")
        ) != expected_types:
            raise AssertionError("pull_request types differ or a path filter was added")

        permissions = _direct_mapping(_mapping_block(text, "permissions", 0))
        if {key: _value(value) for key, value in permissions.items()} != {
            "contents": "read", "pull-requests": "read"
        }:
            raise AssertionError("workflow permissions must be read-only and exact")

        jobs = _direct_mapping(_mapping_block(text, "jobs", 0))
        if frozenset(jobs) != frozenset({"check"}):
            raise AssertionError("workflow must expose exactly one job")
        job = named_job(text, "check")
        job_entries = _direct_mapping(job)
        if frozenset(job_entries) != frozenset({
            "name", "runs-on", "timeout-minutes", "steps"
        }):
            raise AssertionError(f"check job keys drifted: {sorted(job_entries)}")
        expected_job = {
            "name": "review-record-check", "runs-on": "ubuntu-latest", "timeout-minutes": "5"
        }
        for key, expected in expected_job.items():
            if _value(job_entries[key]) != expected:
                raise AssertionError(f"check job {key} drifted")

        steps = _ordered_job_steps(text, "check")
        if len(steps) != 4:
            raise AssertionError(f"expected four steps, found {len(steps)}")

        checkout = _direct_mapping(steps[0])
        if frozenset(checkout) != frozenset({"name", "uses", "with"}):
            raise AssertionError("trusted checkout keys drifted")
        if _value(checkout["name"]) != "Check out trusted base":
            raise AssertionError("trusted checkout must be first")
        if _value(checkout["uses"]) != CHECKOUT_ACTION:
            raise AssertionError("checkout action SHA drifted")
        with_entries = _direct_mapping(_mapping_block(steps[0], "with", 8))
        if {key: _value(value) for key, value in with_entries.items()} != {
            "ref": "${{ github.event.pull_request.base.sha }}",
            "persist-credentials": "false",
        }:
            raise AssertionError("checkout must use the trusted base without credentials")

        _exact_run_step(steps[1], "Witness trusted base", (
            "set -euo pipefail",
            'test "$(git rev-parse HEAD)" = "${{ github.event.pull_request.base.sha }}"',
        ))

        self_test = _direct_mapping(steps[2])
        if frozenset(self_test) != frozenset({"name", "shell", "run"}):
            raise AssertionError("checker self-test step keys drifted")
        if (_value(self_test["name"]), _value(self_test["shell"]),
                _value(self_test["run"])) != (
            "Checker self-test", "bash",
            "python3 scripts/ci/assay_review_record_check.py --self-test",
        ):
            raise AssertionError("checker self-test invocation drifted")

        live = _direct_mapping(steps[3])
        if frozenset(live) != frozenset({"name", "shell", "env", "run"}):
            raise AssertionError("live checker step keys drifted")
        if _value(live["name"]) != "Check exact-head review record" or _value(
            live["shell"]
        ) != "bash":
            raise AssertionError("live checker name/shell drifted")
        env = _direct_mapping(_mapping_block(steps[3], "env", 8))
        if {key: _value(value) for key, value in env.items()} != {
            "GITHUB_TOKEN": "${{ github.token }}",
            "GITHUB_REPOSITORY": "${{ github.repository }}",
            "PR_NUMBER": "${{ github.event.pull_request.number }}",
        }:
            raise AssertionError("live checker environment drifted")
        if tuple(_active_run_lines(steps[3])) != (
            "set -euo pipefail",
            'python3 scripts/ci/assay_review_record_check.py --pr "$PR_NUMBER"',
        ):
            raise AssertionError("live checker invocation drifted")
    except (AssertionError, KeyError) as exc:
        return [str(exc).strip() or "review-record workflow contract failed"]
    return []


def _root_problems(text: str, *, job_name: str, predecessor: str, label: str) -> list[str]:
    try:
        steps = _ordered_job_steps(text, job_name)
        names = [_step_name(step) for step in steps]
        if names.count(ROOT_STEP) != 1 or names.count(predecessor) != 1:
            raise AssertionError(f"{label} root callsite missing or duplicated")
        index = names.index(ROOT_STEP)
        if index == 0 or names[index - 1] != predecessor:
            raise AssertionError(f"{label} root must immediately follow {predecessor}")
        _exact_run_step(steps[index], ROOT_STEP, ROOT_SCRIPT)
    except (AssertionError, KeyError) as exc:
        return [str(exc).strip() or f"{label} required-root contract failed"]
    return []


def contract_problems(workflow: str, ci: str, host: str) -> list[str]:
    return (
        _workflow_problems(workflow)
        + _root_problems(
            host, job_name="check",
            predecessor="Verify required CI aggregator scheduling", label="host-capability")
        + _root_problems(
            ci, job_name="ci",
            predecessor="Verify this gate waits on every gating job", label="CI aggregator")
    )


def main() -> int:
    if len(sys.argv) != 1:
        print("FAIL: this checker accepts no arguments", file=sys.stderr)
        return 2
    problems = contract_problems(
        WORKFLOW.read_text(encoding="utf-8"),
        CI_WORKFLOW.read_text(encoding="utf-8"),
        HOST_WORKFLOW.read_text(encoding="utf-8"),
    )
    if problems:
        for problem in problems:
            print(f"FAIL: {problem}", file=sys.stderr)
        return 1
    print("ok   review-record workflow contract")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
