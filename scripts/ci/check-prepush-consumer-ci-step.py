#!/usr/bin/env python3
"""Pin that required CI runs the two pre-push consumer tests, in one exact step form.

The step sits in ci.yml's `ci` job, whose display name `CI` is a required context. This check is
called by test-ci-hardening-b1.sh, which the separate "Verify CI hardening contracts" step runs,
so removing or disabling the consumer step fails required CI even though that step itself no
longer runs. A test that guarded its own wiring from inside the step could not do that.

Presence of the command lines is not enough: `if false; then ... fi` around them keeps every line
present and runs nothing. Recognising keys is not enough either: a quoted `"if": false` or a
`<<:` merge key is YAML a key pattern does not see, and GitHub would honour it. So the whole step,
from its `- name:` line to the next step, must equal one exact block, byte for byte; only blank
lines and full-line comments at step indentation, which belong to the next step, are ignored at
its end. This is a shape pin over the workflow text, sliced the way test-ci-hardening-b1.sh slices
the hardening step. It is not a shell or YAML interpreter, and it says nothing about what the
commands do; the consumer tests themselves prove that.
"""

import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / ".github/workflows/ci.yml"

HEADING = "      - name: Pre-push hook consumers (Linux check coverage, clippy scope)\n"
INSTALLER = "      - name: Editor-release hook pre-commit consumer\n"
EVALUATE = "      - name: Evaluate required job results\n"
STEP = (
    HEADING
    + "        shell: bash\n"
    + "        run: |\n"
    + "          set -euo pipefail\n"
    + '          export PATH="$HOME/.local/bin:$PATH"\n'
    + "          bash scripts/ci/test-linux-check-precommit-consumer.sh\n"
    + "          bash scripts/ci/test-cargo-clippy-lightweight-scope.sh\n"
)
TRAILING = re.compile(r"^(?: *| {0,6}#.*)$")


def ci_job(text):
    match = re.search(r"(?ms)^  ci:\n(.*?)(?=^  [a-zA-Z][\w-]*:|\Z)", text)
    return None if match is None else match.group(1)


def step_body(section, heading):
    start = section.find(heading)
    if start < 0:
        return None, -1
    rest = section[start + len(heading):]
    nxt = re.search(r"(?m)^      - ", rest)
    return (rest if nxt is None else rest[: nxt.start()]), start


def problems(text):
    jobs = len(re.findall(r"(?m)^  [\"']?ci[\"']?:", text))
    if jobs != 1:
        return [f"want exactly one ci job key in ci.yml, found {jobs}"]
    section = ci_job(text)
    if section is None:
        return ["ci job missing from ci.yml"]
    found = []
    if not re.search(r"(?m)^    name: CI$", section):
        found.append("the ci job is not named CI, the required context")
    count = section.count(HEADING)
    if count != 1:
        return found + [f"want exactly one consumer step in the ci job, found {count}"]
    body, start = step_body(section, HEADING)
    lines = (HEADING + body).splitlines(keepends=True)
    while lines and TRAILING.match(lines[-1].rstrip("\n")):
        lines.pop()
    block = "".join(lines)
    if block != STEP:
        found.append("consumer step must be exactly this block:\n" + STEP + "but is:\n" + block)
    installer, at = step_body(section, INSTALLER)
    if installer is None or "python -m pip install --user pre-commit==" not in installer or at > start:
        found.append("consumer step must follow the step that installs pre-commit")
    if section.find(EVALUATE) < start:
        found.append("consumer step must come before Evaluate required job results")
    return found


def mutate(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"self-test anchor occurs {text.count(old)} times, want 1: {old!r}")
    return text.replace(old, new, 1)


def self_test(text):
    h1 = "          bash scripts/ci/test-linux-check-precommit-consumer.sh\n"
    h2 = "          bash scripts/ci/test-cargo-clippy-lightweight-scope.sh\n"
    shell = HEADING + "        shell: bash\n"
    body, _ = step_body(ci_job(text) or "", HEADING)
    whole_step = HEADING + (body or "")
    cases = {
        "commands wrapped in if false": mutate(text, h1 + h2, "          if false; then\n" + h1 + h2 + "          fi\n"),
        "step removed": mutate(text, whole_step, ""),
        "one command removed": mutate(text, h2, ""),
        "command neutralised with || true": mutate(text, h2, h2.rstrip("\n") + " || true\n"),
        "command commented out": mutate(text, h1, "          # " + h1.lstrip()),
        "early exit before the commands": mutate(text, h1, "          exit 0\n" + h1),
        "step-level if: false": mutate(text, shell, shell + "        if: false\n"),
        "quoted if key": mutate(text, shell, shell + '        "if": false\n'),
        "single-quoted continue-on-error key": mutate(text, shell, shell + "        'continue-on-error': true\n"),
        "merge key carrying if": mutate(text, shell, shell + "        <<: {if: false}\n"),
        "step-level continue-on-error": mutate(text, shell, shell + "        continue-on-error: true\n"),
        "extra env key": mutate(text, shell, shell + "        env:\n          X: y\n"),
        "shell is sh": mutate(text, shell, HEADING + "        shell: sh\n"),
        "step duplicated": mutate(text, whole_step, whole_step + whole_step),
        "installer step renamed": mutate(text, INSTALLER, INSTALLER.replace("consumer", "consumers (renamed)")),
        "ci job renamed": mutate(text, "  ci:\n    name: CI\n", "  ci:\n    name: CI (renamed)\n"),
        "second ci job key": text + '\n  "ci":\n    name: CI\n    runs-on: ubuntu-24.04\n    steps:\n      - run: "true"\n',
    }
    failures = []
    if problems(text):
        failures.append(f"no-op control: the real workflow is refused: {problems(text)}")
    for name, mutated in cases.items():
        if not problems(mutated):
            failures.append(f"mutant accepted: {name}")
    for failure in failures:
        print(f"FAIL: {failure}")
    print(f"self-test: 1 control and {len(cases)} mutants executed")
    return 1 if failures else 0


def main(argv):
    text = CI.read_text(encoding="utf-8")
    if argv == ["--self-test"]:
        return self_test(text)
    if argv:
        print("usage: check-prepush-consumer-ci-step.py [--self-test]", file=sys.stderr)
        return 2
    found = problems(text)
    for problem in found:
        print(f"FAIL: {problem}")
    if not found:
        print("required CI runs both pre-push consumer tests in the pinned step form")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
