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
MAX_BLANK_LINES_BEFORE_LIST = 1
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
    """The bullet list immediately under the anchor.

    "Immediately" is the load-bearing word. Scanning forward until some bullet list turns up
    means that deleting the list under the anchor silently promotes the next list in the
    document to being the required set, and the guard reports agreement about the wrong
    paragraph. So: blank lines may separate the anchor from its list, and the first
    non-blank line that is not a bullet ends the search -- whether or not anything was found.
    """
    lines = text.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == CI_CONTRACT_ANCHOR)
    except StopIteration:
        raise DriftError(
            f"{CI_CONTRACT}: anchor {CI_CONTRACT_ANCHOR!r} is gone; the guard cannot "
            "locate the required set"
        ) from None
    found: list[str] = []
    blanks_before_list = 0
    for line in lines[start + 1 :]:
        match = BULLET_RE.match(line)
        if match:
            # Names carry a trailing parenthetical gloss in this list; keep the name only.
            found.append(match.group(1).strip())
        elif not line.strip() and not found and blanks_before_list < MAX_BLANK_LINES_BEFORE_LIST:
            # One blank line is the markdown convention between a lead-in and its list. Skipping
            # blanks without a bound reopens the hole the anchor is supposed to close: delete the
            # list *and* the paragraph after it and the scan walks on to the next list instead of
            # reporting that its own list is gone.
            blanks_before_list += 1
        else:
            break
    if not found:
        raise DriftError(
            f"{CI_CONTRACT}: no bullet list directly under anchor {CI_CONTRACT_ANCHOR!r}"
        )
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


def _drop_list_under_anchor(text: str) -> str:
    """Remove only the bullets directly under the anchor, keeping the anchor and later lists.

    Later lists have to survive, or the mutation cannot tell the two parsers apart: strip every
    bullet in the file and even a parser that scans onward finds nothing and reports correctly.
    The case worth testing is the one where there is still something further down to grab.
    """
    lines = text.splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.strip() == CI_CONTRACT_ANCHOR)
    cursor = start + 1
    while cursor < len(lines) and not lines[cursor].strip():
        cursor += 1
    end = cursor
    while end < len(lines) and BULLET_RE.match(lines[end]):
        end += 1
    return "".join(lines[: start + 1] + lines[end:])


def _strand_anchor_above_a_foreign_list(text: str) -> str:
    """Leave the anchor followed by blank lines and then somebody else's list.

    The shape a bounded blank-line skip is for: the list under the anchor is gone along with
    the prose that separated it from the next one, so the only thing an unbounded scan finds
    is a list about something else. It must report a missing list, not adopt that one.
    """
    lines = text.splitlines(keepends=True)
    start = next(i for i, line in enumerate(lines) if line.strip() == CI_CONTRACT_ANCHOR)
    nxt = next(i for i in range(start + 1, len(lines)) if BULLET_RE.match(lines[i]))
    # Two blanks: one is the convention the parser allows, so the mutant needs to exceed it.
    return "".join(lines[: start + 1] + ["\n", "\n"] + lines[nxt:])


def self_test() -> int:
    """A guard nobody has seen fail is a guard nobody knows works."""
    baseline = {p: read_repo(p) for p in (RULESET, CI_CONTRACT, RUNBOOK)}
    if check(baseline.__getitem__):
        print("self-test: the repository is already drifting; fix that first", file=sys.stderr)
        return 1

    # Each case names the outcome it requires, because "something went red" is the weakest
    # assertion available and it is how a guard ends up right by accident. A deleted list
    # must be reported as a missing list; if it surfaces as a mismatch instead, the parser
    # walked on and adopted a different paragraph, which is the bug and not the detection.
    mismatch_cases = [
        (CI_CONTRACT, lambda t: t.replace("- `host-capability-check`", "- `retired-context`", 1)),
        # Inside the array specifically: a name in the runbook's prose is out of scope by
        # design, so a mutation there proves nothing about the guard either way.
        (RUNBOOK, lambda t: t.replace('"CI", "host-capability-check"', '"CI"', 1)),
        (RULESET, lambda t: t.replace('"host-capability-check"', '"retired-context"', 1)),
    ]
    unreadable_cases = [
        (CI_CONTRACT, lambda t: t.replace(CI_CONTRACT_ANCHOR, "Contexts, probably:", 1)),
        (RUNBOOK, lambda t: t.replace('"contexts"', '"former_contexts"')),
        (CI_CONTRACT, _drop_list_under_anchor),
        (CI_CONTRACT, _strand_anchor_above_a_foreign_list),
    ]

    def apply(path: Path, mutate) -> dict | None:
        mutated = dict(baseline)
        mutated[path] = mutate(baseline[path])
        if mutated[path] == baseline[path]:
            print(f"self-test: mutation for {path} did not apply", file=sys.stderr)
            return None
        return mutated

    for path, mutate in mismatch_cases:
        mutated = apply(path, mutate)
        if mutated is None:
            return 1
        try:
            problems = check(mutated.__getitem__)
        except DriftError as exc:
            print(f"self-test: drift in {path} was unreadable, expected a mismatch: {exc}",
                  file=sys.stderr)
            return 1
        if not problems:
            print(f"self-test: drift in {path} went undetected", file=sys.stderr)
            return 1

    for path, mutate in unreadable_cases:
        mutated = apply(path, mutate)
        if mutated is None:
            return 1
        try:
            check(mutated.__getitem__)
        except DriftError:
            continue
        print(
            f"self-test: an unreadable {path} did not raise; the guard located a set it "
            "should not have been able to find",
            file=sys.stderr,
        )
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
