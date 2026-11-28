#!/usr/bin/env python3
"""Pin that required CI runs the pre-push consumer tests and the mutation isolation suites.

The aggregate steps are pinned in one exact form: the pre-push consumer step, and the step that runs
the two fast suites proving that the mutation tests never write the source they test.
Golden-path/historical and seal-checker batteries also have exact steps and owner headers.
Their existing jobs, not the near-full CI aggregate, execute these additional batteries.

The step sits in ci.yml's `ci` job, whose display name `CI` is a required context. This check is
called by test-ci-hardening-b1.sh, which the separate "Verify CI hardening contracts" step runs,
so removing or disabling the consumer step fails required CI even though that step itself no
longer runs. A test that guarded its own wiring from inside the step could not do that.

Presence of the command lines is not enough: `if false; then ... fi` around them keeps every line
present and runs nothing. Recognising keys is not enough either: a quoted `"if": false` or a
`<<:` merge key is YAML a key pattern does not see, and GitHub would honour it. So the whole step,
from its `- name:` line to the next step, must equal one exact block, byte for byte; only blank
lines and full-line comments at step indentation, which belong to the next step, are ignored at
its end. The ci job itself ends where YAML ends it, at the next line indented two spaces or less,
so a sibling job written above the steps cannot take them over whatever its key looks like. This
is a shape pin over the workflow text, sliced the way test-ci-hardening-b1.sh slices the hardening
step. It is not a shell or YAML interpreter, and it says nothing about what the commands do; the
consumer tests themselves prove that.
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
# A job written just above the installer owns, in YAML, every step below it, and its `if: false`
# skips them all. Each key spelling below must end the ci job.
SIBLING = "    name: Optional isolation\n    if: false\n    runs-on: ubuntu-24.04\n    steps:\n"
SIBLING_KEYS = ('"optional-isolation":', "'optional-isolation':", "_optional-isolation:", "optional-isolation :")


def job_section(text, name):
    # YAML ends the job where a line is indented no deeper than its key. So any line at two spaces
    # or less that is not a comment ends it, however its key is spelled (quoted, `_job`, `job :`).
    match = re.search(rf"(?ms)^  {re.escape(name)}:\n(.*?)(?=^ {{0,2}}[^\s#]|\Z)", text)
    return None if match is None else match.group(1)


def ci_job(text):
    return job_section(text, "ci")


def step_body(section, heading):
    start = section.find(heading)
    if start < 0:
        return None, -1
    rest = section[start + len(heading):]
    nxt = re.search(r"(?m)^      - ", rest)
    return (rest if nxt is None else rest[: nxt.start()]), start


GOLDEN_JOB = "published-release-golden-path-contract"
GOLDEN_HEADING = "      - name: Remaining golden-path and historical fixture contracts\n"
GOLDEN_STEP = (
    GOLDEN_HEADING
    + "        shell: bash\n"
    + "        run: |\n"
    + "          set -euo pipefail\n"
    + '          pin="$(bash scripts/ci/read-pre-commit-pin.sh)"\n'
    + '          python3 -m pip install --user "pre-commit==${pin}"\n'
    + '          export PATH="$HOME/.local/bin:$PATH"\n'
    + "          pre-commit --version\n"
    + "          python3 scripts/ci/test_golden_path_hook_stages.py\n"
    + "          bash scripts/ci/test-agent-golden-path-skill-optimization.sh\n"
    + "          bash scripts/ci/test-agent-golden-path-skill-hardening.sh\n"
    + "          bash scripts/ci/test-published-release-historical-retention-contract.sh\n"
)
SEAL_HEADING = "      - name: AEE seal checker self-tests (Linux)\n"
SEAL_STEP = (
    SEAL_HEADING
    + "        if: runner.os == 'Linux'\n"
    + "        shell: bash\n"
    + "        run: |\n"
    + "          set -euo pipefail\n"
    + "          python3 scripts/experiments/aee_landlock_seal_fixture.py --meta-test\n"
    + "          python3 scripts/experiments/aee_landlock_seal_fixture.py --required-field-test\n"
    + "          python3 scripts/experiments/aee_landlock_seal_fixture.py --rule-coverage-test\n"
    + "          python3 scripts/experiments/aee_landlock_seal_fixture.py --producer-vocabulary-test\n"
)
# Test-owned headers: no optional job, ignored failure or narrower scope may hide these steps.
# Blank lines and whole-line comments are irrelevant; other syntax needs deliberate review.
BATTERY_HEADERS = {
    "published-release-golden-path-contract": """    name: Published release golden-path contract
    runs-on: ubuntu-24.04
    timeout-minutes: 10
    permissions:
      contents: read
""",
    "test": """    name: Build + Test (${{ matrix.os }})
    needs: [scope]
    if: needs.scope.outputs.lightweight_only != 'true'
    runs-on: ${{ matrix.os }}
    timeout-minutes: ${{ matrix.os == 'windows-latest' && 30 || 20 }}
    permissions:
      contents: read
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-24.04, macos-latest, windows-latest]
    env:
      PYO3_USE_ABI3_FORWARD_COMPATIBILITY: 1
      CARGO_PROFILE_DEV_DEBUG: 0
      CARGO_PROFILE_TEST_DEBUG: 0
""",
}
BATTERIES = ((GOLDEN_JOB, GOLDEN_HEADING, GOLDEN_STEP), ("test", SEAL_HEADING, SEAL_STEP))


def battery_problems(text):
    found = []
    for job, heading, expected in BATTERIES:
        if len(re.findall(rf"(?m)^  [\"']?{re.escape(job)}[\"']?\s*:", text)) != 1:
            found.append(f"want one {job} job")
        section = job_section(text, job)
        if section is None:
            found.append(f"missing battery owner {job}")
            continue
        header, separator, steps = section.partition("    steps:\n")
        # YAML job keys can follow the step sequence too. A header-only pin misses
        # e.g. a trailing continue-on-error. This closed shape keeps all owner keys
        # in the pinned header; only deeper-indented steps/comments follow it.
        if not separator or re.search(r"(?m)^ {0,4}[^\s#]", steps):
            found.append(f"{job} battery owner keys must precede its steps")
        header = "".join(line for line in header.splitlines(keepends=True)
                         if line.strip() and not line.lstrip().startswith("#"))
        if header != BATTERY_HEADERS[job]:
            found.append(f"{job} battery owner header changed")
        if section.count(heading) != 1:
            found.append(f"want exactly one battery step in {job}")
            continue
        body, at = step_body(section, heading)
        lines = (heading + body).splitlines(keepends=True)
        while lines and TRAILING.match(lines[-1].rstrip("\n")):
            lines.pop()
        if "".join(lines) != expected:
            found.append(f"{job} battery step must keep its exact executable form")
        setup = section.find("      - name: Set up Python\n")
        if setup < 0 or setup > at:
            found.append(f"{job} battery must follow Python setup")
    return found


def problems(text):
    jobs = len(re.findall(r"(?m)^  [\"']?ci[\"']?:", text))
    if jobs != 1:
        return [f"want exactly one ci job key in ci.yml, found {jobs}"]
    section = ci_job(text)
    if section is None:
        return ["ci job missing from ci.yml"]
    found = battery_problems(text)
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
    # The installer reads the one pin (scripts/ci/read-pre-commit-pin.sh) before installing it.
    installs = (installer is not None and 'pin="$(bash scripts/ci/read-pre-commit-pin.sh)"' in installer
                and 'python -m pip install --user "pre-commit==${pin}"' in installer)
    if not installs or at > start:
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


def step_cases(text, heading, step, label, job="ci"):
    """The same mutants for every pinned step: each one must be refused."""
    commands = [line for line in step.split("        run: |\n", 1)[1].splitlines(keepends=True) if not line.lstrip().startswith("set ")]
    shell = step.split("        run: |\n", 1)[0]
    body, _ = step_body(job_section(text, job) or "", heading)
    whole_step = heading + (body or "")
    def inside(old, new):
        return mutate(text, step, mutate(step, old, new))

    cases = {
        "commands wrapped in if false": inside("".join(commands), "          if false; then\n" + "".join(commands) + "          fi\n"),
        "step removed": mutate(text, whole_step, ""),
        "early exit before the commands": inside(commands[0], "          exit 0\n" + commands[0]),
        "step-level if: false": inside(shell, shell + "        if: false\n"),
        "quoted if key": inside(shell, shell + '        "if": false\n'),
        "single-quoted continue-on-error key": inside(shell, shell + "        'continue-on-error': true\n"),
        "merge key carrying if": inside(shell, shell + "        <<: {if: false}\n"),
        "step-level continue-on-error": inside(shell, shell + "        continue-on-error: true\n"),
        "extra env key": inside(shell, shell + "        env:\n          X: y\n"),
        "shell is sh": inside(shell, shell.replace("shell: bash", "shell: sh")),
        "inert shell": inside(shell, shell.replace("shell: bash", "shell: echo {0}")),
        "step duplicated": mutate(text, whole_step, whole_step + whole_step),
    }
    for command in commands:
        name = command.strip().split("/")[-1]
        cases[f"command removed ({name})"] = inside(command, "")
        cases[f"command neutralised with || true ({name})"] = inside(command, command.rstrip("\n") + " || true\n")
        cases[f"command commented out ({name})"] = inside(command, "          # " + command.lstrip())
    return {f"{label}: {name}": mutated for name, mutated in cases.items()}


def ownership_cases(text):
    """Sibling jobs that take the pinned steps (refused), and two no-op controls (accepted)."""
    taken = {f"sibling job {key} takes the pinned steps": mutate(text, INSTALLER, f"  {key}\n" + SIBLING + INSTALLER) for key in SIBLING_KEYS}
    controls = {
        "comments at job and file indentation inside the ci job": mutate(text, INSTALLER, "  # note\n# note\n" + INSTALLER),
        "the same sibling job after the ci job": text + f"\n  {SIBLING_KEYS[0]}\n" + SIBLING + '      - run: "true"\n',
    }
    return taken, controls


def self_test(text):
    # Every mutant is built from the real workflow, so the real workflow must pass first.
    if problems(text):
        print(f"FAIL: no-op control refused: the real workflow: {problems(text)}")
        return 1
    cases = {}
    for heading, step, label in PINNED:
        cases.update(step_cases(text, heading, step, label))
    for job, heading, step in BATTERIES:
        cases.update(step_cases(text, heading, step, job, job))
        cases[f"{job}: owner disabled"] = mutate(text, f"  {job}:\n", f"  {job}:\n    if: false\n")
        cases[f"{job}: owner ignores errors"] = mutate(text, f"  {job}:\n", f"  {job}:\n    continue-on-error: true\n")
        owner = job_section(text, job)
        for suffix in ('continue-on-error: true', '"continue-on-error": true',
                       "'continue-on-error': true", '<<: {continue-on-error: true}',
                       'if: false', 'steps: []'):
            cases[f"{job}: trailing owner {suffix}"] = mutate(
                text, owner, owner.rstrip("\n") + "\n    " + suffix + "\n\n")
        cases[f"{job}: optional sibling takes the step"] = mutate(text, heading,
            f"  optional-battery:\n" + SIBLING + heading)
    section = ci_job(text) or ""
    consumer = HEADING + step_body(section, HEADING)[0]
    isolation = ISOLATION + step_body(section, ISOLATION)[0]
    cases.update({
        "isolation step moved before the consumer step": mutate(text, consumer + isolation, isolation + consumer),
        "installer step renamed": mutate(text, INSTALLER, INSTALLER.replace("consumer", "consumers (renamed)")),
        "ci job renamed": mutate(text, "  ci:\n    name: CI\n", "  ci:\n    name: CI (renamed)\n"),
        "second ci job key": text + '\n  "ci":\n    name: CI\n    runs-on: ubuntu-24.04\n    steps:\n      - run: "true"\n',
    })
    taken, controls = ownership_cases(text)
    failures = []
    for name, control in controls.items():
        if problems(control):
            failures.append(f"no-op control refused: {name}: {problems(control)}")
    for name, mutated in cases.items():
        if not problems(mutated):
            failures.append(f"mutant accepted: {name}")
    # Refused for the right reason: both pinned steps left the ci job.
    for name, mutated in taken.items():
        missing = [f"want exactly one {label} in the ci job, found 0" for _, _, label in PINNED]
        if not all(message in problems(mutated) for message in missing):
            failures.append(f"mutant not refused as a job takeover: {name}: {problems(mutated)}")
    for failure in failures:
        print(f"FAIL: {failure}")
    print(f"self-test: {len(controls) + 1} controls and {len(cases) + len(taken)} mutants executed")
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
