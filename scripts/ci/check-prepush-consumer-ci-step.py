#!/usr/bin/env python3
"""Pin that required CI runs the pre-push consumer tests and the mutation isolation suites.

Two steps are pinned, each in one exact form: the pre-push consumer step, and the step that runs
the two fast suites proving that the mutation tests never write the source they test.

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
ISOLATION = "      - name: Mutation tests never write the source (scratch isolation suites)\n"
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
# Fixture repositories and stub cargo only, so seconds and no build; no token in its environment.
ISOLATION_STEP = (
    ISOLATION
    + "        shell: bash\n"
    + "        run: |\n"
    + "          set -euo pipefail\n"
    + "          python3 scripts/ci/test_mutation_scratch_isolation.py\n"
    + "          python3 scripts/ci/test_aee_seal_producer_mutation_safety.py\n"
)
PINNED = ((HEADING, STEP, "consumer step"), (ISOLATION, ISOLATION_STEP, "isolation step"))
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
    starts = {}
    for heading, step, label in PINNED:
        count = section.count(heading)
        if count != 1:
            found.append(f"want exactly one {label} in the ci job, found {count}")
            continue
        body, starts[label] = step_body(section, heading)
        lines = (heading + body).splitlines(keepends=True)
        while lines and TRAILING.match(lines[-1].rstrip("\n")):
            lines.pop()
        block = "".join(lines)
        if block != step:
            found.append(f"{label} must be exactly this block:\n" + step + "but is:\n" + block)
    if len(starts) != len(PINNED):
        return found
    start = starts["consumer step"]
    installer, at = step_body(section, INSTALLER)
    if installer is None or "python -m pip install --user pre-commit==" not in installer or at > start:
        found.append("consumer step must follow the step that installs pre-commit")
    if starts["isolation step"] < start:
        found.append("isolation step must follow the consumer step")
    if section.find(EVALUATE) < max(starts.values()):
        found.append("both pinned steps must come before Evaluate required job results")
    return found


def mutate(text, old, new):
    if text.count(old) != 1:
        raise SystemExit(f"self-test anchor occurs {text.count(old)} times, want 1: {old!r}")
    return text.replace(old, new, 1)


def step_cases(text, heading, step, label):
    """The same mutants for every pinned step: each one must be refused."""
    commands = [line for line in step.splitlines(keepends=True)[3:] if not line.lstrip().startswith(("set ", "export "))]
    shell = heading + "        shell: bash\n"
    body, _ = step_body(ci_job(text) or "", heading)
    whole_step = heading + (body or "")
    cases = {
        "commands wrapped in if false": mutate(text, "".join(commands), "          if false; then\n" + "".join(commands) + "          fi\n"),
        "step removed": mutate(text, whole_step, ""),
        "early exit before the commands": mutate(text, commands[0], "          exit 0\n" + commands[0]),
        "step-level if: false": mutate(text, shell, shell + "        if: false\n"),
        "quoted if key": mutate(text, shell, shell + '        "if": false\n'),
        "single-quoted continue-on-error key": mutate(text, shell, shell + "        'continue-on-error': true\n"),
        "merge key carrying if": mutate(text, shell, shell + "        <<: {if: false}\n"),
        "step-level continue-on-error": mutate(text, shell, shell + "        continue-on-error: true\n"),
        "extra env key": mutate(text, shell, shell + "        env:\n          X: y\n"),
        "shell is sh": mutate(text, shell, heading + "        shell: sh\n"),
        "inert shell": mutate(text, shell, heading + "        shell: echo {0}\n"),
        "step duplicated": mutate(text, whole_step, whole_step + whole_step),
    }
    for command in commands:
        name = command.strip().split("/")[-1]
        cases[f"command removed ({name})"] = mutate(text, command, "")
        cases[f"command neutralised with || true ({name})"] = mutate(text, command, command.rstrip("\n") + " || true\n")
        cases[f"command commented out ({name})"] = mutate(text, command, "          # " + command.lstrip())
    return {f"{label}: {name}": mutated for name, mutated in cases.items()}


def self_test(text):
    cases = {}
    for heading, step, label in PINNED:
        cases.update(step_cases(text, heading, step, label))
    section = ci_job(text) or ""
    consumer = HEADING + step_body(section, HEADING)[0]
    isolation = ISOLATION + step_body(section, ISOLATION)[0]
    cases.update({
        "isolation step moved before the consumer step": mutate(text, consumer + isolation, isolation + consumer),
        "installer step renamed": mutate(text, INSTALLER, INSTALLER.replace("consumer", "consumers (renamed)")),
        "ci job renamed": mutate(text, "  ci:\n    name: CI\n", "  ci:\n    name: CI (renamed)\n"),
        "second ci job key": text + '\n  "ci":\n    name: CI\n    runs-on: ubuntu-24.04\n    steps:\n      - run: "true"\n',
    })
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
        print("required CI runs the pre-push consumer tests and the isolation suites in the pinned step forms")
    return 1 if found else 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
