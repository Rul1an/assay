#!/usr/bin/env python3
"""Required CI runs the ADR-045 seal producer mutations whenever they could have lost a bite.

The mutation battery proves that every seeded defect in crates/assay-cli/src/aee_seal.rs is caught
by a named test. It ran only as a pre-push hook, so no required check proved it. This pins the
narrow CI route: the `scope` job sets `aee_seal_mutation_touched` from the changed paths, the
`aee-seal-producer-mutations` job runs only then, and the `CI` gate requires it exactly then.

The trigger is the battery's inputs (the producer source, the round-trip tests that catch two of
its mutants, main.rs where that test module is registered, the harness and its scratch library)
plus this file. This file pins the job, so any change to the job's wiring has to change this file
too, and that change runs the job. A plain `workflow_dispatch` has no diff and runs it
(fail-closed); a docs-only dispatch provably cannot touch these paths. The pre-push hook selects
the same battery inputs.

The scope decision is tested by executing the real `detect` body (review finding AEE-ROUTE-2):
stub `git` and `gh` hand it a controlled change set, the real classifier and drift helper run, and
the published GITHUB_OUTPUT is read back. Reading the grep or counting assignments is not enough:
a later assignment before publication would change what is published while every text check holds.

The workflow and the hook configuration are parsed (Ruby's YAML.safe_load, aliases off, the
repository precedent), never matched as text, except inside a `run:` body, which is shell.
"""

from __future__ import annotations

import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

ROOT = Path(__file__).resolve().parents[2]
CI = ROOT / ".github/workflows/ci.yml"
CONFIG = ROOT / ".pre-commit-config.yaml"
JOB = "aee-seal-producer-mutations"
OUTPUT = "aee_seal_mutation_touched"
HOOK = "aee-seal-producer-mutations"

# What the battery reads: a change to any of these can remove a bite. main.rs registers
# `#[cfg(test)] mod aee_seal_round_trip;`; dropping that drops two named detectors (AEE-ROUTE-1).
BATTERY_INPUTS = (
    "crates/assay-cli/src/aee_seal.rs",
    "crates/assay-cli/src/aee_seal_round_trip.rs",
    "crates/assay-cli/src/main.rs",
    "scripts/ci/test_aee_seal_producer_mutations.py",
    "scripts/ci/test_aee_seal_producer_mutation_safety.py",
    "scripts/ci/lib/mutation_scratch.py",
)
# What wires the battery into CI: changing the job means changing its pin, here.
WIRING = ("scripts/ci/test_aee_seal_mutation_ci_route.py",)
UNRELATED = (
    "crates/assay-cli/src/aee_seal_key.rs",
    "crates/assay-cli/src/aee_seal.rs.orig",
    "crates/assay-cli/src/main.rs.orig",
    "scripts/ci/lib/process_group.py",
    ".github/workflows/ci.yml",
    "docs/aee_seal.rs",
)
BASE, HEAD, BEFORE = "b" * 40, "c" * 40, "d" * 40
EXPRESSIONS = {
    "${{ github.event.pull_request.base.sha }}": BASE,
    "${{ github.sha }}": HEAD,
    "${{ github.event.before }}": BEFORE,
}
# The stubs answer exactly the calls the detect body makes, and fail loudly on any other.
STUB_GIT = """#!/bin/sh
case "$1" in
  diff|diff-tree) printf '%s' "$STUB_CHANGED" ;;
  *) echo "stub git: unexpected $*" >&2; exit 97 ;;
esac
"""
STUB_GH = """#!/bin/sh
case "$*" in
  *"/pulls/$DISPATCH_PR_NUMBER/files"*) printf '%s' "$STUB_PR_FILES" ;;
  *"/pulls/$DISPATCH_PR_NUMBER"*) echo "$EXPECTED_HEAD_SHA" ;;
  *) echo "stub gh: unexpected $*" >&2; exit 97 ;;
esac
"""

MUTATION_RUN = (
    "set -euo pipefail\n"
    "python3 -B scripts/ci/test_aee_seal_producer_mutation_safety.py\n"
    "python3 -B scripts/ci/test_aee_seal_producer_mutations.py\n"
)
TRIPLE = f'"{JOB}|${{AEE_SEAL_PRODUCER_MUTATIONS_RESULT}}|${{aee_expectation}}"'
EXPECTATION = (
    'if [[ "${AEE_SEAL_MUTATION_TOUCHED}" == "false" ]]; then\n'
    '  aee_expectation="optional"\n'
    "else\n"
    '  aee_expectation="required"\n'
    "fi\n"
)


def load(path: Path):
    raw = subprocess.check_output(
        ["ruby", "-rjson", "-ryaml", "-e",
         "puts JSON.generate(YAML.safe_load(STDIN.read, aliases: false))"],
        input=path.read_bytes(),
    )
    return json.loads(raw)


def step_named(job: dict, name: str) -> dict:
    steps = [step for step in job.get("steps", []) if step.get("name") == name]
    if len(steps) != 1:
        raise AssertionError(f"want one step named {name!r}, found {len(steps)}")
    return steps[0]


def detect_run(workflow: dict) -> str:
    steps = [s for s in workflow["jobs"]["scope"]["steps"] if s.get("id") == "detect"]
    if len(steps) != 1:
        raise AssertionError("want exactly one `detect` step in the scope job")
    return steps[0]["run"]


def run_detect(workflow: dict, event: str, changed: tuple[str, ...] = (), *, dispatch_pr: str = "",
               before: str = BEFORE, pr_files: tuple[str, ...] = ()) -> tuple[list[str], str]:
    """Execute the real `detect` body; return its published output lines and its summary."""
    body = detect_run(workflow)
    for expression, value in EXPRESSIONS.items():
        body = body.replace(expression, value if expression != "${{ github.event.before }}" else before)
    if "${{" in body:
        raise AssertionError("the detect body uses an expression this test does not supply")
    with tempfile.TemporaryDirectory(prefix="aee-route-detect-") as temporary:
        base = Path(temporary)
        stubs = base / "bin"
        stubs.mkdir()
        for name, text in (("git", STUB_GIT), ("gh", STUB_GH)):
            (stubs / name).write_text(text)
            (stubs / name).chmod(0o755)
        env = {key: value for key, value in os.environ.items()
               if not key.startswith(("GIT_", "GITHUB_", "PRE_COMMIT", "RUNNER_"))}
        env.update(
            PATH=f"{stubs}{os.pathsep}{os.environ['PATH']}", PYTHONDONTWRITEBYTECODE="1",
            GITHUB_EVENT_NAME=event, GITHUB_SHA=HEAD, GITHUB_REPOSITORY="example/assay",
            DISPATCH_PR_NUMBER=dispatch_pr, EXPECTED_HEAD_SHA=HEAD, GH_TOKEN="unused",
            RUNNER_TEMP=str(base), GITHUB_OUTPUT=str(base / "output"),
            GITHUB_STEP_SUMMARY=str(base / "summary"),
            STUB_CHANGED="".join(path + "\n" for path in changed),
            STUB_PR_FILES="".join(path + "\n" for path in pr_files),
        )
        result = subprocess.run(["bash", "-c", body], cwd=ROOT, env=env, capture_output=True,
                                text=True, timeout=60, stdin=subprocess.DEVNULL)
        if result.returncode != 0:
            raise AssertionError(f"detect body exited {result.returncode}: {result.stderr}")
        return (base / "output").read_text().splitlines(), (base / "summary").read_text()


def published(lines: list[str]) -> str:
    values = [line.split("=", 1)[1] for line in lines if line.startswith(f"{OUTPUT}=")]
    if len(values) != 1:
        raise AssertionError(f"want {OUTPUT} published exactly once, got {values}")
    return values[0]


def dedent(block: str) -> str:
    lines = block.splitlines(keepends=True)
    indent = min(len(line) - len(line.lstrip()) for line in lines if line.strip())
    return "".join(line[indent:] for line in lines)


class ScopeOutput(unittest.TestCase):
    def setUp(self) -> None:
        self.workflow = load(CI)
        self.run = detect_run(self.workflow)

    def test_the_scope_job_declares_the_output_from_the_detect_step(self) -> None:
        outputs = self.workflow["jobs"]["scope"]["outputs"]
        self.assertEqual(outputs.get(OUTPUT), f"${{{{ steps.detect.outputs.{OUTPUT} }}}}")

    def test_a_pull_request_publishes_true_exactly_for_the_inputs_and_the_pin(self) -> None:
        for path, want in [(p, "true") for p in BATTERY_INPUTS + WIRING] + [(p, "false") for p in UNRELATED]:
            with self.subTest(path=path):
                lines, _ = run_detect(self.workflow, "pull_request", (path,))
                self.assertEqual(published(lines), want)

    def test_one_input_among_unrelated_paths_is_enough(self) -> None:
        lines, summary = run_detect(self.workflow, "pull_request",
                                    UNRELATED[:3] + ("crates/assay-cli/src/main.rs",) + UNRELATED[3:])
        self.assertEqual(published(lines), "true")
        self.assertIn(f"- {OUTPUT}: true", summary.splitlines())

    def test_a_push_publishes_the_same_on_both_range_routes(self) -> None:
        for before in (BEFORE, "0" * 40):  # an ordinary push, and a new branch (diff-tree)
            for path, want in (("crates/assay-cli/src/aee_seal.rs", "true"), ("README.md", "false")):
                with self.subTest(before=before[:1], path=path):
                    lines, _ = run_detect(self.workflow, "push", (path,), before=before)
                    self.assertEqual(published(lines), want)

    def test_a_dispatch_without_a_diff_fails_closed_and_runs_it(self) -> None:
        lines, _ = run_detect(self.workflow, "workflow_dispatch")
        self.assertEqual(published(lines), "true")

    def test_a_docs_only_dispatch_cannot_touch_the_battery(self) -> None:
        lines, _ = run_detect(self.workflow, "workflow_dispatch", dispatch_pr="7",
                              pr_files=("docs/README.md",))
        self.assertEqual(published(lines), "false")


class Job(unittest.TestCase):
    def test_the_job_has_exactly_this_shape(self) -> None:
        jobs = load(CI)["jobs"]
        self.assertIn(JOB, jobs)
        apt = step_named(jobs["clippy"], "Install Linux deps (for build scripts)")["run"]
        checkout = jobs["clippy"]["steps"][0]
        self.assertEqual(set(checkout), {"uses", "with"})
        expected = {
            "name": "ADR-045 seal producer mutations",
            "needs": ["scope"],
            "if": f"needs.scope.outputs.{OUTPUT} == 'true'",
            "runs-on": "ubuntu-24.04",
            "timeout-minutes": 30,
            "permissions": {"contents": "read"},
            "steps": [
                checkout,
                {"name": "Set up Rust toolchain and cache", "uses": "./.github/actions/setup-rust"},
                {"name": "Install Linux deps (for build scripts)", "shell": "bash", "run": apt},
                {
                    "name": "Every seeded producer defect is caught",
                    "shell": "bash",
                    # The harness reads cargo's test output as text; never let a runner default color it.
                    "env": {"CARGO_TERM_COLOR": "never"},
                    "run": MUTATION_RUN,
                },
            ],
        }
        self.assertEqual(jobs[JOB], expected)


class Gate(unittest.TestCase):
    def setUp(self) -> None:
        self.ci = load(CI)["jobs"]["ci"]
        self.step = step_named(self.ci, "Evaluate required job results")

    def test_the_gate_waits_for_the_job(self) -> None:
        self.assertEqual(self.ci["needs"].count(JOB), 1)

    def test_the_gate_reads_the_result_and_the_scope_output(self) -> None:
        env = self.step["env"]
        self.assertEqual(env.get("AEE_SEAL_PRODUCER_MUTATIONS_RESULT"), f"${{{{ needs.{JOB}.result }}}}")
        self.assertEqual(env.get("AEE_SEAL_MUTATION_TOUCHED"), f"${{{{ needs.scope.outputs.{OUTPUT} }}}}")

    def test_only_a_literal_false_relaxes_the_job(self) -> None:
        run = self.step["run"]
        start = run.index('if [[ "${AEE_SEAL_MUTATION_TOUCHED}"')
        block = run[start:run.index("fi\n", start) + 3]
        self.assertEqual(dedent(block), EXPECTATION)
        self.assertEqual(run.count(TRIPLE), 1)


class Hook(unittest.TestCase):
    def test_the_pre_push_hook_selects_every_battery_input(self) -> None:
        hooks = [h for repo in load(CONFIG)["repos"] for h in repo.get("hooks", []) if h.get("id") == HOOK]
        self.assertEqual(len(hooks), 1)
        hook = hooks[0]
        self.assertEqual(hook.get("stages"), ["pre-push"])
        selector = re.compile(hook["files"])
        for path in BATTERY_INPUTS:
            with self.subTest(path=path):
                self.assertTrue(selector.search(path), f"{path} can remove a bite but does not run the battery")
        for path in WIRING + UNRELATED:
            with self.subTest(path=path):
                self.assertFalse(selector.search(path))


if __name__ == "__main__":
    unittest.main()
