#!/usr/bin/env python3
"""Pin the required branch-protection contexts to one value across every place that names them.

Three files describe the same set: the importable ruleset, the CI contract, and the
branch-protection runbook. Nothing reconciled them, so an edit to one left the others
describing a set that no longer exists. PR #1878 retired the bare `lane-check` context in
three documents and missed these, and the stale name survived in an artifact whose whole
purpose is to be imported into live protection.

The guard is the mechanism that a written "remember to update the others" is not. It reads
each location through an explicit anchor and fails when the anchor is missing, so
restructuring a document surfaces as an error rather than as a check that quietly stops
looking.

Scope, so the green result is not read as more than it is: this compares the *structured*
statements of the set -- the ruleset entries, the anchored bullet list, and the
`"contexts": [...]` arrays an operator would copy. Surrounding prose is deliberately not
matched, because the runbook legitimately names `lane-check` as the informational job it
still is, and a guard that cannot tell that from a stale requirement would be turned off.

Usage:
    check-required-contexts.py             # verify the three locations agree
    check-required-contexts.py --self-test # prove each parser detects a drift
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

RULESET = Path(".github/rulesets/main-required-ci-contexts.json")
CI_CONTRACT = Path("CI-CONTRACT.md")
RUNBOOK = Path("docs/BRANCH-PROTECTION-SETUP.md")

CI_CONTRACT_ANCHOR = "Currently required live branch-protection contexts:"
BULLET_RE = re.compile(r"^-\s+`([^`]+)`")
CONTEXTS_ARRAY_RE = re.compile(r'"contexts"\s*:\s*(\[[^\]]*\])')


class DriftError(Exception):
    """A location disagrees with the ruleset, or its anchor is gone."""


def ruleset_contexts(text: str) -> list[str]:
    """The importable artifact is the source of truth: it is the one that can be applied."""
    doc = json.loads(text)
    found: list[str] = []
    for rule in doc.get("rules", []):
        if rule.get("type") != "required_status_checks":
            continue
        for check in rule.get("parameters", {}).get("required_status_checks", []):
            found.append(check["context"])
    if not found:
        raise DriftError(
            f"{RULESET}: no required_status_checks rule found; the guard cannot "
            "determine the required set"
        )
    return found


def ci_contract_contexts(text: str) -> list[str]:
    """The bullet list under the anchor, read until the list ends."""
    lines = text.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == CI_CONTRACT_ANCHOR)
    except StopIteration:
        raise DriftError(
            f"{CI_CONTRACT}: anchor {CI_CONTRACT_ANCHOR!r} is gone; the guard cannot "
            "locate the required set"
        ) from None
    found: list[str] = []
    for line in lines[start + 1 :]:
        match = BULLET_RE.match(line)
        if match:
            # Names carry a trailing parenthetical gloss in this list; keep the name only.
            found.append(match.group(1).strip())
        elif found:
            break
    if not found:
        raise DriftError(f"{CI_CONTRACT}: the list under the anchor is empty")
    return found


def runbook_contexts(text: str) -> list[list[str]]:
    """Every `"contexts": [...]` array in the runbook, each of which must match."""
    arrays = [json.loads(m.group(1)) for m in CONTEXTS_ARRAY_RE.finditer(text)]
    if not arrays:
        raise DriftError(
            f'{RUNBOOK}: no `"contexts": [...]` array found; the guard cannot locate '
            "the required set"
        )
    return arrays


def compare(expected: list[str], actual: list[str], where: str) -> list[str]:
    """Set comparison, because these three lists are legitimately ordered differently."""
    if set(expected) == set(actual):
        return []
    missing = sorted(set(expected) - set(actual))
    extra = sorted(set(actual) - set(expected))
    detail = []
    if missing:
        detail.append(f"missing {missing}")
    if extra:
        detail.append(f"unexpected {extra}")
    return [f"{where}: {', '.join(detail)} (ruleset requires {sorted(expected)})"]


def check(read) -> list[str]:
    expected = ruleset_contexts(read(RULESET))
    problems = compare(expected, ci_contract_contexts(read(CI_CONTRACT)), str(CI_CONTRACT))
    for index, array in enumerate(runbook_contexts(read(RUNBOOK))):
        problems += compare(expected, array, f"{RUNBOOK} contexts array #{index + 1}")
    return problems


def read_repo(path: Path) -> str:
    return (REPO_ROOT / path).read_text(encoding="utf-8")


def self_test() -> int:
    """A guard nobody has seen fail is a guard nobody knows works."""
    baseline = {p: read_repo(p) for p in (RULESET, CI_CONTRACT, RUNBOOK)}
    if check(baseline.__getitem__):
        print("self-test: the repository is already drifting; fix that first", file=sys.stderr)
        return 1

    cases = [
        (CI_CONTRACT, lambda t: t.replace("- `host-capability-check`", "- `retired-context`", 1)),
        # Inside the array specifically: a name in the runbook's prose is out of scope by
        # design, so a mutation there proves nothing about the guard either way.
        (RUNBOOK, lambda t: t.replace('"CI", "host-capability-check"', '"CI"', 1)),
        (RULESET, lambda t: t.replace('"host-capability-check"', '"retired-context"', 1)),
        (CI_CONTRACT, lambda t: t.replace(CI_CONTRACT_ANCHOR, "Contexts, probably:", 1)),
        (RUNBOOK, lambda t: t.replace('"contexts"', '"former_contexts"')),
    ]
    for path, mutate in cases:
        mutated = dict(baseline)
        mutated[path] = mutate(baseline[path])
        if mutated[path] == baseline[path]:
            print(f"self-test: mutation for {path} did not apply", file=sys.stderr)
            return 1
        try:
            problems = check(mutated.__getitem__)
        except DriftError:
            continue
        if not problems:
            print(f"self-test: drift in {path} went undetected", file=sys.stderr)
            return 1
    print("check-required-contexts self-test=passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove the parsers detect drift")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    try:
        problems = check(read_repo)
    except DriftError as exc:
        print(f"required-contexts=failed\n{exc}", file=sys.stderr)
        return 1

    if problems:
        print("required-contexts=failed", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "\nThe required-context set is named in three places and they must agree. "
            "The ruleset is the source of truth because it is the one that can be imported.",
            file=sys.stderr,
        )
        return 1

    print("required-contexts=passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
