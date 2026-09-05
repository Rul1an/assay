#!/usr/bin/env python3
"""Repository-wide policy for `continue-on-error` in GitHub workflows.

`continue-on-error: true` makes a failure not fail its job. That is legitimate for advisory work
and for a step whose failure is not the thing under test, and it is a fail-open route everywhere
else. Nothing checked it outside `ci.yml` (#2348).

This is deliberately NOT an extension of `check-ci-gate-coverage.py`. That checker answers a
different question -- does every gating job in `ci.yml` reach the required `CI` rollup and get
judged fail-closed -- and it needs that rollup's topology to do it. The other workflows have no such
aggregator, so broadening it by swapping a constant would silently change what it means. One rule,
one checker.

The rule:

* A **job-level** `continue-on-error: true` is refused unless the workflow is declared advisory in
  ALLOWED_JOB_LEVEL, with a reason. A whole job that cannot fail is a job whose result nobody reads.
* A **step-level** `continue-on-error: true` is refused unless that step is in ALLOWED_STEP_LEVEL,
  with a reason. Step-level use is ordinarily fine -- an artifact upload should not fail a run whose
  real work already passed -- but "ordinarily fine" is not a thing a checker can assert, so each one
  is named.

Both lists carry a reason per entry rather than a bare path. An allowlist without reasons is a
grandfather clause: it records that something was permitted, not why, so nobody can later tell an
intentional exemption from one that was pasted in to make a red go away.

Fails closed. An unreadable file, a `continue-on-error` this parser cannot attribute to a job or a
step, an allowlist entry naming something that no longer exists, and any spelling other than a bare
`true`/`false` are all errors rather than passes.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

WORKFLOW_DIR = Path(".github/workflows")

# workflow -> reason. Every job in these workflows may carry `continue-on-error: true`.
ALLOWED_JOB_LEVEL: dict[str, str] = {
    "adr025-nightly-evidence.yml": (
        "Nightly informational evidence. The jobs are named '(informational)' and publish a "
        "readiness signal; a red one is a datum, not a gate."
    ),
    "perf_pr.yml": (
        "Criterion compare is advisory on a PR: benchmark noise must not block a merge. The "
        "enforcing perf signal is Bencher's threshold on main, not this job."
    ),
    "split-wave0-gates.yml": (
        "The Wave 0.1 nightly safety job is an explicit non-blocking stub; its own name says so."
    ),
    "wave6-nightly-safety.yml": (
        "Nightly Miri and property smoke. Both are long-running exploratory checks whose failures "
        "are investigated rather than merge-blocking."
    ),
}

# (workflow, step name) -> reason.
ALLOWED_STEP_LEVEL: dict[tuple[str, str], str] = {
    ("action-v2-test.yml", "Run action with missing pack (expected failure)"): (
        "The step failing IS the assertion; the following step checks the outcome."
    ),
    ("action-v2-test.yml", "Run action with invalid pack (expected failure)"): (
        "The step failing IS the assertion; the following step checks the outcome."
    ),
    ("action-v2-test.yml", "Review with required mode and no bundles"): (
        "The step failing IS the assertion; the following step "
        "'Assert required mode fails without bundles' checks the outcome."
    ),
    ("action-v2-test.yml", "Review corrupted bundle with required mode"): (
        "The step failing IS the assertion; the following step "
        "'Assert corrupted bundle is refused' checks the outcome."
    ),
    ("adr025-nightly-evidence.yml", "Download current soak artifact"): (
        "A first run has no prior artifact to download; absence is expected, not a failure."
    ),
    ("adr025-nightly-evidence.yml", "Download current readiness artifact"): (
        "A first run has no prior artifact to download; absence is expected, not a failure."
    ),
    ("assay-security.yml", "Validate (SARIF)"): (
        "SARIF upload is reporting, not gating. The security verdict is the scan's own exit code."
    ),
    ("ci.yml", "Upload verification logs"): (
        "Diagnostics for a run that has already reached its verdict. Losing the upload must not "
        "change that verdict."
    ),
    ("fuzz-smoke.yml", "Upload crash artifacts"): (
        "Runs only when a crash was found; the fuzz step has already failed the job by then."
    ),
    ("osv-scanner-scheduled.yml", "Run OSV scanner"): (
        "Scheduled advisory scan. A scanner outage must not page anyone; findings are read from "
        "the uploaded report."
    ),
    ("smoke-install.yml", "Publish test report"): (
        "Reporting step after the smoke assertions have already decided the job."
    ),
}

BOOL_RE = re.compile(r"^(?P<indent>\s*)continue-on-error:\s*(?P<value>\S+)\s*(?:#.*)?$")
NAME_RE = re.compile(r"^\s*-?\s*name:\s*(?P<name>.+?)\s*$")
JOB_INDENT = 4
STEP_MIN_INDENT = 8


class PolicyError(Exception):
    """The policy could not be evaluated, or was violated."""


def _step_name(lines: list[str], index: int) -> str | None:
    """The `name:` of the step or job the flag at `index` belongs to.

    Walks backwards to the nearest `name:`. A `continue-on-error` with no name above it in the file
    cannot be attributed, and is an error rather than a pass.
    """
    for j in range(index, -1, -1):
        match = NAME_RE.match(lines[j])
        if match:
            return match.group("name").strip("\"'")
    return None


def scan(path: Path) -> list[tuple[str, str, str]]:
    """(kind, name, raw-value) for every `continue-on-error` in one workflow."""
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise PolicyError(f"{path}: unreadable, so the policy cannot be evaluated: {error}") from None

    found = []
    for index, line in enumerate(lines):
        # A commented flag sets nothing. Stripping comments first would also swallow a real flag
        # whose value carries a trailing comment, so the comment case is matched, not removed.
        if line.lstrip().startswith("#"):
            continue
        match = BOOL_RE.match(line)
        if not match:
            continue
        value = match.group("value")
        if value not in {"true", "false"}:
            raise PolicyError(
                f"{path}:{index + 1}: `continue-on-error: {value}` is not a bare true/false. "
                "GitHub accepts other spellings and expressions; this policy does not read them, "
                "so it refuses rather than guessing."
            )
        if value == "false":
            continue
        indent = len(match.group("indent"))
        name = _step_name(lines, index)
        if name is None:
            raise PolicyError(
                f"{path}:{index + 1}: `continue-on-error: true` has no `name:` above it, so it "
                "cannot be attributed to a job or step."
            )
        kind = "step" if indent >= STEP_MIN_INDENT else "job"
        found.append((kind, name, value))
    return found


def main() -> int:
    if not WORKFLOW_DIR.is_dir():
        print(f"FAIL: {WORKFLOW_DIR} is not a directory", file=sys.stderr)
        return 2

    violations: list[str] = []
    seen_jobs: set[str] = set()
    seen_steps: set[tuple[str, str]] = set()

    for path in sorted(WORKFLOW_DIR.glob("*.yml")):
        try:
            entries = scan(path)
        except PolicyError as error:
            print(f"FAIL: {error}", file=sys.stderr)
            return 2
        for kind, name, _ in entries:
            if kind == "job":
                if path.name not in ALLOWED_JOB_LEVEL:
                    violations.append(
                        f"{path.name}: job '{name}' sets continue-on-error: true. A job that "
                        "cannot fail is a job whose result nobody reads. Add the workflow to "
                        "ALLOWED_JOB_LEVEL with a reason, or remove the flag."
                    )
                seen_jobs.add(path.name)
            else:
                key = (path.name, name)
                if key not in ALLOWED_STEP_LEVEL:
                    violations.append(
                        f"{path.name}: step '{name}' sets continue-on-error: true and is not in "
                        "ALLOWED_STEP_LEVEL. Add it with a reason, or remove the flag."
                    )
                seen_steps.add(key)

    # An allowlist entry for something that no longer exists is a stale permission. It reads as
    # review having happened when it has not, and it hides the entry that replaced it.
    for workflow in sorted(set(ALLOWED_JOB_LEVEL) - seen_jobs):
        violations.append(
            f"{workflow}: allowlisted for job-level continue-on-error but has none. Remove the "
            "stale entry."
        )
    for workflow, step in sorted(set(ALLOWED_STEP_LEVEL) - seen_steps):
        violations.append(
            f"{workflow}: step '{step}' is allowlisted but not present. Remove the stale entry."
        )

    if violations:
        for violation in violations:
            print(f"FAIL: {violation}", file=sys.stderr)
        return 1

    print(
        f"workflow continue-on-error policy: {len(seen_jobs)} advisory workflow(s), "
        f"{len(seen_steps)} named step(s), each with a stated reason."
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
