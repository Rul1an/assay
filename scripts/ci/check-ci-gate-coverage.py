#!/usr/bin/env python3
"""Assert the required `CI` rollup waits on every job `ci.yml` defines to gate a merge.

Two jobs added to block merges -- `publish-shape-cli` (#1333) and `public-crate-policy`
(#1335) -- were never added to the rollup's `needs:`, which dates to #455. They ran, they
could go red, and the only required context stayed green regardless, so both guardrails
reported without blocking for eleven weeks (#2230).

The rule is derived from the workflow rather than restated beside it. A hardcoded list of
job names would be a second statement of the same rule, free to drift from the first, which
is the defect this closes reintroduced one level up. So: every job the workflow defines
either reaches the rollup, or carries an opt-out marker a human had to type with a reason
attached. A job added tomorrow is covered by construction, because it starts with neither.

Reaching the rollup means both halves. `needs:` membership alone only makes the rollup wait;
the rollup then decides on a table of `name|result|expectation` triples, and a job absent
from that table is waited on and then ignored. Membership without a triple is therefore an
error, as is a triple reading another job's result variable.

Expectations are checked too, because a permissive one is a fail-open that survives correct
wiring: a job with no `if:` can only legitimately end in `success`, so its expectation must
be the literal `required`. Only a job that can be scoped out may carry a computed one.

`continue-on-error: true` (also YAML `True` / `TRUE`) is the same class of fail-open one road
over (#2242): a step failure becomes a successful conclusion, so the rollup reads `success`
while the guardrail is red. Every such key in a wired job — at job indent (4 spaces) or step
indent (8 spaces) only, never deeper run/heredoc text — is rejected unless the same step
carries `# ci-gate: continue-on-error-ok --` with a reason at least as long as the job opt-out
floor. Job-level `continue-on-error` has no marker and is always a problem.

The aggregator is not named here either. It is whichever job in the workflow reports a
context the checked-in ruleset requires -- so renaming that job without amending the ruleset
fails this check rather than silently detaching branch protection.

Anything this cannot parse is an error, never a pass. A restructured workflow surfaces as a
failure to read it, not as a guard that quietly stopped looking.

Usage:
    check-ci-gate-coverage.py             # verify the rollup covers every gating job
    check-ci-gate-coverage.py --self-test # prove the guard detects each way it can be defeated
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[2]

WORKFLOW = Path(".github/workflows/ci.yml")
RULESET = Path(".github/rulesets/main-required-ci-contexts.json")

# A marker a human must type, with a reason on the same line. The length floor is not about
# prose quality: it is there so the marker cannot be added as reflex punctuation while a
# reviewer skims past it.
MARKER_PREFIX = "# ci-gate: not-required --"
# Step-level twin: a wired job may keep `continue-on-error: true` only when the same step
# carries this marker with a real reason. Job-level `continue-on-error` has no opt-out.
CONTINUE_ON_ERROR_MARKER_PREFIX = "# ci-gate: continue-on-error-ok --"
MIN_REASON_CHARS = 30

JOB_KEY_RE = re.compile(r"^  (?P<name>[A-Za-z0-9_][A-Za-z0-9_-]*):\s*(?:#.*)?$")
JOB_NAME_RE = re.compile(r"^    name:\s*(?P<value>.+?)\s*$")
JOB_IF_RE = re.compile(r"^    if:\s*\S")
MARKER_RE = re.compile(rf"^    {re.escape(MARKER_PREFIX)}\s*(?P<reason>\S.*)$")
CONTINUE_ON_ERROR_MARKER_RE = re.compile(
    rf"^        {re.escape(CONTINUE_ON_ERROR_MARKER_PREFIX)}\s*(?P<reason>\S.*)$"
)
# Only the two semantic key indents: job properties are four spaces, step keys eight.
# Deeper indentation is run/heredoc payload text, not a GitHub Actions key.
CONTINUE_ON_ERROR_TRUE_RE = re.compile(
    r"^(?P<indent>    |        )continue-on-error:\s*(?:true|True|TRUE)\b"
)
STEP_START_RE = re.compile(r"^      - ")
NEEDS_RE = re.compile(r"^    needs:\s*\[(?P<items>[^\]]*)\]\s*$")
RESULT_ENV_RE = re.compile(
    r"^\s*(?P<var>[A-Z][A-Z0-9_]*):\s*\$\{\{\s*needs\.(?P<job>[A-Za-z0-9_-]+)\.result\s*\}\}\s*$"
)
TRIPLE_RE = re.compile(
    r'^\s*"(?P<job>[A-Za-z0-9_-]+)\|\$\{(?P<var>[A-Z][A-Z0-9_]*)\}\|(?P<expectation>[^"]*)"'
)
COMPUTED_EXPECTATION_RE = re.compile(r"^\$\{[a-z_]+\}$")


class CoverageError(Exception):
    """The workflow or the ruleset could not be read the way this guard needs to read it."""


def required_contexts(ruleset_text: str) -> set[str]:
    doc = json.loads(ruleset_text)
    found = {
        check["context"]
        for rule in doc.get("rules", [])
        if rule.get("type") == "required_status_checks"
        for check in rule.get("parameters", {}).get("required_status_checks", [])
    }
    if not found:
        raise CoverageError(f"{RULESET}: no required_status_checks rule; no required set to read")
    return found


def parse_jobs(lines: list[str]) -> dict[str, list[str]]:
    """Every top-level job, mapped to the lines of its block, comments included.

    Line-based on purpose: a YAML loader would drop the opt-out markers, which are comments.
    """
    try:
        start = next(i for i, line in enumerate(lines) if line.rstrip() == "jobs:")
    except StopIteration:
        raise CoverageError(f"{WORKFLOW}: no top-level `jobs:` mapping") from None
    end = next(
        (i for i in range(start + 1, len(lines)) if lines[i][:1] not in ("", " ", "#")),
        len(lines),
    )

    keys = [i for i in range(start + 1, end) if JOB_KEY_RE.match(lines[i])]
    if not keys:
        raise CoverageError(f"{WORKFLOW}: `jobs:` defines nothing this guard can recognise")
    jobs = {}
    for position, index in enumerate(keys):
        name = JOB_KEY_RE.match(lines[index])["name"]  # type: ignore[index]
        stop = keys[position + 1] if position + 1 < len(keys) else end
        jobs[name] = lines[index:stop]
    return jobs


def display_name(body: list[str]) -> str | None:
    for line in body:
        match = JOB_NAME_RE.match(line)
        if match:
            return match["value"].strip("\"'")
    return None


def find_aggregator(jobs: dict[str, list[str]], contexts: set[str]) -> str:
    matches = [name for name, body in jobs.items() if display_name(body) in contexts]
    if len(matches) == 1:
        return matches[0]
    if not matches:
        raise CoverageError(
            f"{WORKFLOW}: no job reports a required context {sorted(contexts)}. Either the "
            "rollup job was renamed, which detaches branch protection, or the ruleset no "
            "longer names the context it reports."
        )
    raise CoverageError(
        f"{WORKFLOW}: {sorted(matches)} all report a required context; the guard cannot tell "
        "which one is the rollup"
    )


def parse_needs(aggregator: str, body: list[str]) -> list[str]:
    for line in body:
        if line.startswith("    needs:"):
            match = NEEDS_RE.match(line)
            if not match:
                raise CoverageError(
                    f"{WORKFLOW}: `{aggregator}` declares `needs:` in a form this guard cannot "
                    "read; keep it a single-line flow list"
                )
            return [item.strip() for item in match["items"].split(",") if item.strip()]
    raise CoverageError(f"{WORKFLOW}: `{aggregator}` has no `needs:`; it waits on nothing")


def result_variables(body: list[str]) -> dict[str, str]:
    """job name -> the environment variable the rollup binds its result to."""
    bound: dict[str, str] = {}
    for line in body:
        match = RESULT_ENV_RE.match(line)
        if match:
            bound[match["job"]] = match["var"]
    return bound


def evaluated_triples(body: list[str]) -> dict[str, tuple[str, str]]:
    """job name -> (result variable, expectation) as the rollup's decision table states it."""
    return {
        match["job"]: (match["var"], match["expectation"])
        for match in (TRIPLE_RE.match(line) for line in body)
        if match
    }


def step_blocks(job_body: list[str]) -> list[list[str]]:
    """Step lists inside a job body, still as raw lines so step markers stay visible.

    Same contract as `parse_jobs`: comments are load-bearing, so this stays line-based.
    """
    blocks: list[list[str]] = []
    current: list[str] | None = None
    for line in job_body:
        if STEP_START_RE.match(line):
            current = [line]
            blocks.append(current)
            continue
        if current is None:
            continue
        if line.startswith("        ") or line == "":
            current.append(line)
        else:
            current = None
    return blocks


def continue_on_error_problems(name: str, job_body: list[str]) -> list[str]:
    """Reject substituted conclusions on wired jobs unless a step marker excuses them."""
    problems: list[str] = []
    covered: set[int] = set()

    for step in step_blocks(job_body):
        continue_idxs = [
            id(line) for line in step if CONTINUE_ON_ERROR_TRUE_RE.match(line)
        ]
        if not continue_idxs:
            continue
        covered.update(continue_idxs)
        marker = next((m for m in map(CONTINUE_ON_ERROR_MARKER_RE.match, step) if m), None)
        if marker is None:
            problems.append(
                f"`{name}` has a step with `continue-on-error: true` unmarked; add "
                f"`{CONTINUE_ON_ERROR_MARKER_PREFIX} <reason>` on that step, or drop the flag "
                "so a failure can redden the required context (#2242)"
            )
            continue
        reason = marker["reason"].strip()
        if len(reason) < MIN_REASON_CHARS:
            problems.append(
                f"`{name}` allows `continue-on-error` with a {len(reason)}-character reason; "
                f"state why a substituted conclusion is acceptable in at least "
                f"{MIN_REASON_CHARS} characters"
            )

    for line in job_body:
        match = CONTINUE_ON_ERROR_TRUE_RE.match(line)
        if not match or id(line) in covered:
            continue
        if match["indent"] == "    ":
            problems.append(
                f"`{name}` sets job-level `continue-on-error: true`, which can keep the "
                "required context green when the job fails (#2242)"
            )
        else:
            problems.append(
                f"`{name}` has `continue-on-error: true` outside a recognisable step; the "
                "guard will not guess which conclusion GitHub substitutes (#2242)"
            )
    return problems


def check(workflow_text: str, ruleset_text: str) -> list[str]:
    lines = workflow_text.splitlines()
    jobs = parse_jobs(lines)
    aggregator = find_aggregator(jobs, required_contexts(ruleset_text))
    body = jobs[aggregator]

    needs = parse_needs(aggregator, body)
    bound = result_variables(body)
    triples = evaluated_triples(body)
    if not triples:
        raise CoverageError(
            f"{WORKFLOW}: `{aggregator}` has no `name|result|expectation` decision table; the "
            "guard cannot tell which results it acts on"
        )

    problems = [
        f"`needs:` names `{name}`, which is not a job in this workflow"
        for name in needs
        if name not in jobs
    ]

    for name, job_body in jobs.items():
        if name == aggregator:
            continue
        marker = next((m for m in map(MARKER_RE.match, job_body) if m), None)
        wired = name in needs

        if marker and wired:
            problems.append(
                f"`{name}` carries the `{MARKER_PREFIX}` marker and is also in `needs:`; one of "
                "the two is a leftover and the guard will not guess which"
            )
            continue
        if marker:
            reason = marker["reason"].strip()
            if len(reason) < MIN_REASON_CHARS:
                problems.append(
                    f"`{name}` opts out with a {len(reason)}-character reason; state why this "
                    f"job cannot gate a merge in at least {MIN_REASON_CHARS} characters"
                )
            continue
        if not wired:
            problems.append(
                f"`{name}` is absent from `{aggregator}`'s `needs:`, so its failure cannot make "
                f"the required context red. Wire it in, or mark it `{MARKER_PREFIX} <reason>` "
                "at the job's own indentation if it is genuinely not a merge gate"
            )
            continue

        problems.extend(continue_on_error_problems(name, job_body))

        variable, expectation = triples.get(name, (None, None))
        if variable is None:
            problems.append(
                f"`{aggregator}` waits on `{name}` and then never judges it: no entry in the "
                "decision table"
            )
            continue
        if bound.get(name) != variable:
            problems.append(
                f"`{name}` is judged on `${{{variable}}}`, which is not the variable bound to "
                f"`needs.{name}.result` ({bound.get(name) or 'nothing'})"
            )
            continue
        if expectation == "required":
            continue
        if not COMPUTED_EXPECTATION_RE.match(expectation):
            problems.append(
                f"`{name}` carries the expectation `{expectation}`, which is neither `required` "
                "nor a computed expectation the gate derives from a scope output"
            )
        elif not any(map(JOB_IF_RE.match, job_body)):
            problems.append(
                f"`{name}` has no `if:`, so it always runs and `success` is its only acceptable "
                f"result, but the gate expects `{expectation}` and would accept a skip"
            )

    return problems


def read_repo(path: Path) -> str:
    return (REPO_ROOT / path).read_text(encoding="utf-8")


def _aggregator_key_index(lines: list[str]) -> int:
    jobs = parse_jobs([line.rstrip("\n") for line in lines])
    aggregator = find_aggregator(jobs, required_contexts(read_repo(RULESET)))
    return next(
        i
        for i, line in enumerate(lines)
        if (match := JOB_KEY_RE.match(line.rstrip("\n"))) and match["name"] == aggregator
    )


def _rewrite_aggregator_needs(replacement: list[str]):
    """Replace the rollup's own `needs:` line.

    Targeted rather than textual: `ebpf-smoke-ubuntu` declares `needs: [scope, test]` earlier in
    the file, so a first-match substitution mutates a job the rollup does not read and the case
    proves nothing.
    """

    def mutate(text: str) -> str:
        lines = text.splitlines(keepends=True)
        at = _aggregator_key_index(lines)
        index = next(i for i in range(at, len(lines)) if lines[i].startswith("    needs:"))
        return "".join(lines[:index] + replacement + lines[index + 1 :])

    return mutate


def _add_job(reason: str | None):
    """Insert a plausible new gating job ahead of the rollup, as a future PR would."""

    def mutate(text: str) -> str:
        lines = text.splitlines(keepends=True)
        block = ["  brand-new-guardrail:\n"]
        if reason is not None:
            block.append(f"    {MARKER_PREFIX} {reason}\n")
        block += [
            "    name: Brand new guardrail\n",
            "    runs-on: ubuntu-latest\n",
            "    steps:\n",
            "      - run: ./scripts/ci/brand-new-guardrail.sh\n",
            "\n",
        ]
        at = _aggregator_key_index(lines)
        return "".join(lines[:at] + block + lines[at:])

    return mutate


def self_test() -> int:
    """A guard nobody has seen fail is a guard nobody knows works."""
    workflow = read_repo(WORKFLOW)
    ruleset = read_repo(RULESET)
    if check(workflow, ruleset):
        print("self-test: the workflow is already failing this check; fix that first", file=sys.stderr)
        return 1

    good_reason = "post-merge only, so it can never report on a pull request"

    # Each case names the message it requires. "Something went red" is the weakest assertion
    # available and it is how a guard ends up correct by accident.
    detected = [
        ("an unwired new job", _add_job(None), "brand-new-guardrail` is absent"),
        ("a new job opting out with no real reason", _add_job("n/a"), "character reason"),
        (
            "a wired job dropped from needs",
            lambda t: t.replace("publish-shape-cli, ", "", 1),
            "publish-shape-cli` is absent",
        ),
        (
            "a wired job removed from the decision table",
            lambda t: re.sub(r'^ *"publish-shape-cli\|.*\n', "", t, count=1, flags=re.M),
            "never judges it",
        ),
        (
            "a decision table entry reading another job's result",
            lambda t: t.replace(
                '"publish-shape-cli|${PUBLISH_SHAPE_CLI_RESULT}',
                '"publish-shape-cli|${CLIPPY_RESULT}',
                1,
            ),
            "not the variable bound",
        ),
        (
            "an unconditional job allowed to be skipped",
            lambda t: t.replace(
                '"publish-shape-cli|${PUBLISH_SHAPE_CLI_RESULT}|required"',
                '"publish-shape-cli|${PUBLISH_SHAPE_CLI_RESULT}|${code_gated_expectation}"',
                1,
            ),
            "would accept a skip",
        ),
        (
            "an expectation that is neither required nor computed",
            lambda t: t.replace(
                '"publish-shape-cli|${PUBLISH_SHAPE_CLI_RESULT}|required"',
                '"publish-shape-cli|${PUBLISH_SHAPE_CLI_RESULT}|optional"',
                1,
            ),
            "neither `required`",
        ),
        (
            "a wired job that also claims the opt-out",
            lambda t: t.replace("  clippy:\n", f"  clippy:\n    {MARKER_PREFIX} {good_reason}\n", 1),
            "is also in `needs:`",
        ),
        (
            "a typo in needs",
            _rewrite_aggregator_needs(["    needs: [scoop, publish-shape-cli]\n"]),
            "not a job in this workflow",
        ),
        (
            "an unmarked continue-on-error on a gating step",
            lambda t: t.replace(
                "      - name: Install Linux deps (for build scripts)\n",
                "      - name: Install Linux deps (for build scripts)\n"
                "        continue-on-error: true\n",
                1,
            ),
            "continue-on-error: true` unmarked",
        ),
        (
            "job-level continue-on-error on a wired job",
            lambda t: t.replace(
                "  publish-shape-cli:\n",
                "  publish-shape-cli:\n    continue-on-error: true\n",
                1,
            ),
            "job-level `continue-on-error: true`",
        ),
        (
            "the diagnostic upload step without its marker",
            lambda t: re.sub(
                rf"^        {re.escape(CONTINUE_ON_ERROR_MARKER_PREFIX)} .*\n",
                "",
                t,
                count=1,
                flags=re.M,
            ),
            "continue-on-error: true` unmarked",
        ),
        (
            "a continue-on-error marker with no real reason",
            lambda t: re.sub(
                rf"^(        {re.escape(CONTINUE_ON_ERROR_MARKER_PREFIX)} ).+$",
                r"\1n/a",
                t,
                count=1,
                flags=re.M,
            ),
            "character reason",
        ),
        (
            "an unmarked continue-on-error: True on a gating step",
            lambda t: t.replace(
                "      - name: Install Linux deps (for build scripts)\n",
                "      - name: Install Linux deps (for build scripts)\n"
                "        continue-on-error: True\n",
                1,
            ),
            "continue-on-error: true` unmarked",
        ),
        (
            "an unmarked continue-on-error: TRUE on a gating step",
            lambda t: t.replace(
                "      - name: Install Linux deps (for build scripts)\n",
                "      - name: Install Linux deps (for build scripts)\n"
                "        continue-on-error: TRUE\n",
                1,
            ),
            "continue-on-error: true` unmarked",
        ),
    ]

    for label, mutate, expected in detected:
        mutated = mutate(workflow)
        if mutated == workflow:
            print(f"self-test: the mutation for {label} did not apply", file=sys.stderr)
            return 1
        try:
            problems = check(mutated, ruleset)
        except CoverageError as exc:
            print(f"self-test: {label} was unreadable, expected a finding: {exc}", file=sys.stderr)
            return 1
        if not any(expected in problem for problem in problems):
            print(
                f"self-test: {label} was not reported as expected ({expected!r}); got {problems}",
                file=sys.stderr,
            )
            return 1

    # The opt-out has to work, or the guard is a rule nobody can satisfy honestly -- and this is
    # what makes the unwired case above a finding about the wiring rather than about newness.
    if check(_add_job(good_reason)(workflow), ruleset):
        print("self-test: a deliberately marked non-gating job was still reported", file=sys.stderr)
        return 1

    # A run/heredoc payload can contain the same characters; only job (4) and step (8) indents
    # are YAML keys. Ten-space script text must not redden the gate.
    script_payload = workflow.replace(
        "          set -euo pipefail\n",
        "          set -euo pipefail\n"
        "          continue-on-error: true\n",
        1,
    )
    if script_payload == workflow:
        print("self-test: the run-payload mutation for continue-on-error did not apply", file=sys.stderr)
        return 1
    if check(script_payload, ruleset):
        print(
            "self-test: a continue-on-error string inside a run payload was reported as a key",
            file=sys.stderr,
        )
        return 1

    unreadable = [
        (
            "a renamed rollup job",
            lambda t: t.replace("    name: CI\n", "    name: CI rollup\n", 1),
            ruleset,
        ),
        ("a rollup with no needs", _rewrite_aggregator_needs([]), ruleset),
        (
            "a needs written as a block list",
            _rewrite_aggregator_needs(["    needs:\n", "      - scope\n"]),
            ruleset,
        ),
        (
            "a ruleset that no longer requires the rollup's context",
            lambda t: t,
            ruleset.replace('"CI"', '"Continuous Integration"', 1),
        ),
    ]
    for label, mutate, rules in unreadable:
        try:
            check(mutate(workflow), rules)
        except CoverageError:
            continue
        print(f"self-test: {label} did not raise; the guard read a structure it should not have",
              file=sys.stderr)
        return 1

    print("ci-gate-coverage self-test=passed")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--self-test", action="store_true", help="prove the guard has teeth")
    args = parser.parse_args()

    if args.self_test:
        return self_test()

    try:
        problems = check(read_repo(WORKFLOW), read_repo(RULESET))
    except CoverageError as exc:
        print(f"ci-gate-coverage=failed\n  {exc}", file=sys.stderr)
        return 1

    if problems:
        print("ci-gate-coverage=failed", file=sys.stderr)
        for problem in problems:
            print(f"  {problem}", file=sys.stderr)
        print(
            "\nA job outside the rollup's `needs:`, or one whose step concludes success via "
            "`continue-on-error`, can fail while the required context reports success, so the "
            "merge it was written to block goes through (#2230, #2242).",
            file=sys.stderr,
        )
        return 1

    print("ci-gate-coverage=passed")
    return 0


if __name__ == "__main__":
    sys.exit(main())
