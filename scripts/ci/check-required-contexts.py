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
    check-required-contexts.py --live-response PATH  # reconcile ruleset vs live API JSON
    check-required-contexts.py --live-response -     # same, reading stdin
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
RECONCILE_WORKFLOW = Path(".github/workflows/required-context-reconciliation.yml")

CI_CONTRACT_ANCHOR = "Currently required live branch-protection contexts:"
CI_CONTRACT_SENTINEL = "<!-- required-contexts:end"
BULLET_RE = re.compile(r"^-\s+`([^`]+)`")
CONTEXTS_ARRAY_RE = re.compile(r'"contexts"\s*:\s*(\[[^\]]*\])')

EXIT_MATCH = 0
EXIT_DRIFT = 1
EXIT_UNREADABLE = 2


class DriftError(Exception):
    """A location disagrees with the ruleset, or its anchor is gone."""


def _ruleset_required_status_parameters(text: str) -> dict:
    """Shared ruleset parser: the required_status_checks rule parameters object."""
    doc = json.loads(text)
    for rule in doc.get("rules", []):
        if rule.get("type") != "required_status_checks":
            continue
        params = rule.get("parameters")
        if not isinstance(params, dict):
            raise DriftError(
                f"{RULESET}: required_status_checks rule has no parameters object"
            )
        return params
    raise DriftError(
        f"{RULESET}: no required_status_checks rule found; the guard cannot "
        "determine the required set"
    )


def ruleset_contexts(text: str) -> list[str]:
    """The importable artifact is the source of truth: it is the one that can be applied."""
    params = _ruleset_required_status_parameters(text)
    found: list[str] = []
    for check in params.get("required_status_checks", []):
        found.append(check["context"])
    if not found:
        raise DriftError(
            f"{RULESET}: no required_status_checks entries; the guard cannot "
            "determine the required set"
        )
    return found


def ruleset_strict(text: str) -> bool:
    """Strictness comes from the same ruleset artifact as the context list."""
    params = _ruleset_required_status_parameters(text)
    if "strict_required_status_checks_policy" not in params:
        raise DriftError(
            f"{RULESET}: missing strict_required_status_checks_policy; the guard "
            "cannot determine the required strict setting"
        )
    value = params["strict_required_status_checks_policy"]
    if not isinstance(value, bool):
        raise DriftError(
            f"{RULESET}: strict_required_status_checks_policy must be a boolean"
        )
    return value


def live_protection(text: str) -> tuple[list[str], bool]:
    """Parse classic branch-protection required_status_checks JSON (contexts + strict).

    app_id on individual checks is ignored on purpose: live and the ruleset already
    disagree there for CI, and this slice reconciles only context set and strictness.
    """
    if not text or not text.strip():
        raise DriftError("live response is empty")
    try:
        doc = json.loads(text)
    except json.JSONDecodeError as exc:
        raise DriftError(f"live response is not JSON: {exc}") from exc
    if not isinstance(doc, dict):
        raise DriftError("live response must be a JSON object")
    missing = [key for key in ("contexts", "strict") if key not in doc]
    if missing:
        raise DriftError(
            "live response missing required_status_checks fields: "
            + ", ".join(missing)
        )
    contexts = doc["contexts"]
    strict = doc["strict"]
    if not isinstance(contexts, list) or not all(isinstance(c, str) for c in contexts):
        raise DriftError("live response contexts must be a list of strings")
    if not isinstance(strict, bool):
        raise DriftError("live response strict must be a boolean")
    return contexts, strict


def check_live(read, live_text: str) -> list[str]:
    """Compare live classic protection to the ruleset (contexts set + strict only)."""
    expected = ruleset_contexts(read(RULESET))
    expected_strict = ruleset_strict(read(RULESET))
    actual, actual_strict = live_protection(live_text)
    problems = compare(expected, actual, "live required_status_checks.contexts")
    if expected_strict != actual_strict:
        problems.append(
            "live required_status_checks.strict="
            f"{actual_strict!r} but ruleset strict_required_status_checks_policy="
            f"{expected_strict!r}"
        )
    return problems


def read_live_response(path: str) -> str:
    if path == "-":
        return sys.stdin.read()
    live_path = Path(path)
    try:
        return live_path.read_text(encoding="utf-8")
    except OSError as exc:
        raise DriftError(f"cannot read live response {path}: {exc}") from exc


def main_live(path: str) -> int:
    try:
        live_text = read_live_response(path)
        problems = check_live(read_repo, live_text)
    except DriftError as exc:
        print(f"required-contexts-live=unreadable\n{exc}", file=sys.stderr)
        return EXIT_UNREADABLE

    if problems:
        print("required-contexts-live=drift", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        return EXIT_DRIFT

    print("required-contexts-live=match")
    return EXIT_MATCH


def run_live_exit(live_text: str, baseline: dict) -> int:
    """Exercise check_live exit mapping without going through argparse/filesystem."""
    try:
        problems = check_live(baseline.__getitem__, live_text)
    except DriftError:
        return EXIT_UNREADABLE
    if problems:
        return EXIT_DRIFT
    return EXIT_MATCH


def ci_contract_contexts(text: str) -> list[str]:
    """The bullets between the anchor and the end sentinel.

    Delimited at both ends rather than inferred from proximity. Proximity cannot express the
    property this needs: once only whitespace separates the anchor from the next list, nothing
    in the text distinguishes "my list" from "somebody else's", so any blank-line budget either
    still adopts the neighbour or rejects the living document. Counting blanks was tried and it
    bought nothing -- at the one blank line a normal deletion leaves behind, a bounded scan and
    an unbounded one behave identically.

    With an explicit end marker the region is stated instead of guessed: a deleted list leaves
    an empty region, a deleted marker leaves no region at all, and neither can reach past the
    marker to borrow a list that belongs to another paragraph.
    """
    lines = text.splitlines()
    try:
        start = next(i for i, line in enumerate(lines) if line.strip() == CI_CONTRACT_ANCHOR)
    except StopIteration:
        raise DriftError(
            f"{CI_CONTRACT}: anchor {CI_CONTRACT_ANCHOR!r} is gone; the guard cannot "
            "locate the required set"
        ) from None
    try:
        end = next(
            i
            for i in range(start + 1, len(lines))
            if lines[i].lstrip().startswith(CI_CONTRACT_SENTINEL)
        )
    except StopIteration:
        raise DriftError(
            f"{CI_CONTRACT}: end sentinel {CI_CONTRACT_SENTINEL!r} is missing after the "
            "anchor; the guard cannot tell where the required set stops"
        ) from None

    # Names carry a trailing parenthetical gloss in this list; keep the name only.
    found = [m.group(1).strip() for m in map(BULLET_RE.match, lines[start + 1 : end]) if m]
    if not found:
        raise DriftError(
            f"{CI_CONTRACT}: no entries between anchor {CI_CONTRACT_ANCHOR!r} and its "
            "end sentinel"
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


def _anchor_index(lines: list[str]) -> int:
    return next(i for i in range(len(lines)) if lines[i].strip() == CI_CONTRACT_ANCHOR)


def _sentinel_index(lines: list[str], start: int) -> int:
    return next(
        i
        for i in range(start + 1, len(lines))
        if lines[i].lstrip().startswith(CI_CONTRACT_SENTINEL)
    )


def _empty_the_region(text: str) -> str:
    """Delete the entries, keeping the anchor and the sentinel.

    The region is still well formed and simply says nothing, which has to be reported as an
    empty region rather than filled in from the neighbourhood.

    This case pins the report; it does not distinguish this design from the proximity scan it
    replaced, because that one also finds nothing here. Recorded rather than left implied: a
    mutant list is only as useful as the cases in it that actually diverge, and the two below
    are the ones that do.
    """
    lines = text.splitlines(keepends=True)
    start = _anchor_index(lines)
    return "".join(lines[: start + 1] + lines[_sentinel_index(lines, start) :])


def _strand_anchor_above_a_foreign_list(text: str) -> str:
    """Delete the entries, the sentinel, and everything up to the next list in the document.

    The shape proximity cannot handle: nothing but whitespace between the anchor and a list
    that belongs to another paragraph. A scan that infers its region from nearness adopts that
    list at any blank-line budget a living document also needs, which is why the region is
    delimited instead. Verified against the reconstructed proximity parser rather than argued.
    """
    lines = text.splitlines(keepends=True)
    start = _anchor_index(lines)
    after_sentinel = _sentinel_index(lines, start) + 1
    nxt = next(i for i in range(after_sentinel, len(lines)) if BULLET_RE.match(lines[i]))
    return "".join(lines[: start + 1] + ["\n"] + lines[nxt:])


def _remove_sentinel(text: str) -> str:
    lines = text.splitlines(keepends=True)
    start = _anchor_index(lines)
    end = _sentinel_index(lines, start)
    # The marker spans two lines; drop the continuation with it so no stray comment text stays.
    stop = end + 1
    while stop < len(lines) and "-->" not in lines[stop - 1]:
        stop += 1
    return "".join(lines[:end] + lines[stop:])


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
        (CI_CONTRACT, _empty_the_region),
        (CI_CONTRACT, _strand_anchor_above_a_foreign_list),
        (CI_CONTRACT, _remove_sentinel),
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

    if live_self_test(baseline) != 0:
        return 1

    if workflow_contract_self_test() != 0:
        return 1

    print("check-required-contexts self-test=passed")
    return 0


def _baseline_live_json(contexts: list[str], *, strict: bool = True, app_id=15368) -> str:
    """Shape of GET .../protection/required_status_checks, including ignored app_id."""
    return json.dumps(
        {
            "strict": strict,
            "contexts": list(contexts),
            "checks": [{"context": c, "app_id": app_id} for c in contexts],
        },
        indent=2,
    )


def live_self_test(baseline: dict) -> int:
    """Prove live-response mode detects semantic drift and unreadable evidence separately."""
    expected = ruleset_contexts(baseline[RULESET])
    expected_strict = ruleset_strict(baseline[RULESET])
    live_ok = _baseline_live_json(expected, strict=expected_strict)

    # Match, including reordered contexts and an app_id that disagrees with the ruleset.
    reordered = list(reversed(expected))
    assert set(reordered) == set(expected)
    for label, live_text in (
        ("baseline", live_ok),
        ("reordered", _baseline_live_json(reordered, strict=expected_strict)),
        ("app_id_ignored", _baseline_live_json(expected, strict=expected_strict, app_id=None)),
    ):
        try:
            problems = check_live(baseline.__getitem__, live_text)
        except DriftError as exc:
            print(f"self-test: live {label} should match, got unreadable: {exc}", file=sys.stderr)
            return 1
        if problems:
            print(f"self-test: live {label} should match, got drift: {problems}", file=sys.stderr)
            return 1

    drift_cases = [
        (
            "context_added",
            lambda t: _baseline_live_json(expected + ["extra-context"], strict=expected_strict),
        ),
        (
            "context_removed",
            lambda t: _baseline_live_json(expected[:-1], strict=expected_strict),
        ),
        (
            "strict_flip",
            lambda t: _baseline_live_json(expected, strict=not expected_strict),
        ),
    ]
    for label, mutate in drift_cases:
        mutated = mutate(live_ok)
        if mutated == live_ok:
            print(f"self-test: live drift mutation {label} did not apply", file=sys.stderr)
            return 1
        try:
            problems = check_live(baseline.__getitem__, mutated)
        except DriftError as exc:
            print(
                f"self-test: live drift {label} was unreadable, expected semantic drift: {exc}",
                file=sys.stderr,
            )
            return 1
        if not problems:
            print(f"self-test: live drift {label} went undetected", file=sys.stderr)
            return 1

    unreadable_live = [
        ("empty", lambda t: ""),
        ("malformed", lambda t: "{not-json"),
        ("missing_contexts", lambda t: json.dumps({"strict": True})),
        ("missing_strict", lambda t: json.dumps({"contexts": expected})),
    ]
    for label, mutate in unreadable_live:
        mutated = mutate(live_ok)
        if mutated == live_ok:
            print(f"self-test: live unreadable mutation {label} did not apply", file=sys.stderr)
            return 1
        try:
            check_live(baseline.__getitem__, mutated)
        except DriftError:
            continue
        print(
            f"self-test: live unreadable {label} did not raise; the guard accepted bad evidence",
            file=sys.stderr,
        )
        return 1

    # Harness bite: a true no-op must be rejected by the mutation applicator, not silent.
    noop = (lambda t: t)(live_ok)
    if noop != live_ok:
        print("self-test: no-op mutation unexpectedly changed live JSON", file=sys.stderr)
        return 1
    # And changing only app_id must not be treated as drift (already covered) while still
    # changing the file so a harness would accept the mutation as applied.
    app_only = _baseline_live_json(expected, strict=expected_strict, app_id=99999)
    if app_only == live_ok:
        print("self-test: app_id-only mutation did not apply", file=sys.stderr)
        return 1
    try:
        problems = check_live(baseline.__getitem__, app_only)
    except DriftError as exc:
        print(f"self-test: app_id-only change must be ignored, got unreadable: {exc}", file=sys.stderr)
        return 1
    if problems:
        print(f"self-test: app_id-only change must be ignored, got drift: {problems}", file=sys.stderr)
        return 1

    # CLI exit codes: 0 match, 1 drift, 2 unreadable.
    if run_live_exit(live_ok, baseline) != EXIT_MATCH:
        print("self-test: live CLI match must exit 0", file=sys.stderr)
        return 1
    if run_live_exit(
        _baseline_live_json(expected + ["extra"], strict=expected_strict), baseline
    ) != EXIT_DRIFT:
        print("self-test: live CLI drift must exit 1", file=sys.stderr)
        return 1
    if run_live_exit("{", baseline) != EXIT_UNREADABLE:
        print("self-test: live CLI unreadable must exit 2", file=sys.stderr)
        return 1

    return 0


def workflow_contract_self_test() -> int:
    """Thin string contracts for the reconciliation workflow — not a YAML parser."""
    path = REPO_ROOT / RECONCILE_WORKFLOW
    if not path.is_file():
        print(f"self-test: missing workflow {RECONCILE_WORKFLOW}", file=sys.stderr)
        return 1
    text = path.read_text(encoding="utf-8")

    required_substrings = [
        "schedule:",
        "workflow_dispatch:",
        "cron:",
        "contents: read",
        "secrets.BRANCH_PROTECTION_READ_TOKEN",
        "check-required-contexts.py --live-response",
        "protection/required_status_checks",
        "github.repository",
        "github.event.repository.default_branch",
        "set -euo pipefail",
    ]
    for needle in required_substrings:
        if needle not in text:
            print(f"self-test: workflow missing {needle!r}", file=sys.stderr)
            return 1

    forbidden = [
        "pull_request:",
        "merge_group:",
        "github.token",
        "GITHUB_TOKEN",
        " -X PUT",
        " -X PATCH",
        " -X DELETE",
        "|| github.token",
        "|| secrets.GITHUB_TOKEN",
    ]
    for needle in forbidden:
        if needle in text:
            print(f"self-test: workflow must not contain {needle!r}", file=sys.stderr)
            return 1

    # Exactly one GET via gh api (no method flag => GET). Count gh api invocations.
    gh_api_lines = [
        line for line in text.splitlines() if re.search(r"(^|[^\w-])gh(\s+|$)api\b", line)
    ]
    if len(gh_api_lines) != 1:
        print(
            f"self-test: workflow must contain exactly one gh api call, found {len(gh_api_lines)}",
            file=sys.stderr,
        )
        return 1
    if re.search(r"-X\s+(PUT|PATCH|DELETE|POST)\b", gh_api_lines[0]):
        print("self-test: the sole gh api call must be a GET", file=sys.stderr)
        return 1

    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove the parsers detect drift")
    parser.add_argument(
        "--live-response",
        metavar="PATH",
        help="reconcile ruleset against live required_status_checks JSON (PATH or - for stdin)",
    )
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    if args.live_response is not None:
        return main_live(args.live_response)

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
