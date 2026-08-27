#!/usr/bin/env python3
"""Mutations for the external Conformance inventory callsite pin.

    python3 scripts/ci/test-conformance-inventory-callsite.py

The inventory tests already encode this contract, but they execute inside the
step they guard. These mutations therefore target the later required-CI
checker and the workflow calls that invoke it. The live workflow is never
written. Command literals here are an independent oracle, not an import.

Hardening and finale check each other under a single-mutation contract:
changing only one root must turn the remaining caller red. The host job
is used as the scheduling root only because it has no if/needs; the CI
root pins that absence. Simultaneous mutation of both required roots
(jobs.ci.if and the host check job if/needs) is outside repo-local
enforcement. This is not self-enforcement against coordinated workflow
edits, and not a hosted-CI proof.
"""

from __future__ import annotations

import hashlib
import importlib.util
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(REPO / "conformance/tests"))

from test_completion_scope import (  # noqa: E402
    TRUSTED_PREFIX_WRITERS,
    _active_run_lines,
    assert_hard_run_command,
    named_step,
)

CHECKER_PATH = REPO / "scripts/ci/check-conformance-inventory-callsite.py"
B1_PATH = REPO / "scripts/ci/test-ci-hardening-b1.sh"
CI_YML = REPO / ".github/workflows/ci.yml"
HOST_YML = REPO / ".github/workflows/host-capability-check.yml"
JOB = "scope"
INVENTORY_STEP = "Conformance inventory"
HARDENING_STEP = "Verify CI hardening contracts"
FINALE_JOB = "ci"
FINALE_STEP = "Verify this gate waits on every gating job"
FINALE_CHECKER = "python3 scripts/ci/check-conformance-inventory-callsite.py"
HOST_JOB = "check"
HOST_STEP = "Verify required CI aggregator scheduling"
# Independent of the checker module and of HOST_SCHEDULE_RUN_SCRIPT.
HOST_SCHEDULE_COMMANDS = (
    "set -euo pipefail",
    "python3 scripts/ci/check-conformance-inventory-callsite.py --required-aggregator-schedule",
)
CI_JOB_IF_BLOCK = (
    "  ci:\n"
    "    name: CI\n"
    "    runs-on: ubuntu-latest\n"
    "    timeout-minutes: 10\n"
    "    if: always()\n"
)

# Independent oracle. Do not import the completion-scope run-script tuple.
REQUIRED_INVENTORY_COMMANDS = (
    "set -euo pipefail",
    'test "$(git rev-parse HEAD)" = "$GITHUB_SHA"',
    "python3 conformance/registry.py",
    "python3 conformance/implementations.py",
    "python3 -W error::ResourceWarning conformance/tests/test_implementations.py",
    "python3 -W error::ResourceWarning conformance/tests/test_pma_v0_registration.py",
    "python3 -W error::ResourceWarning conformance/tests/test_registry.py",
    "python3 -W error::ResourceWarning conformance/tests/test_bounded_run.py",
    "python3 -W error::ResourceWarning conformance/tests/test_run_all.py",
    "python3 -W error::ResourceWarning conformance/tests/test_completion_scope.py",
    "python3 conformance/run_all.py",
)
# Independent of the completion-scope gated-run tuple.
GATED_LINUX_JOB = "test"
GATED_LINUX_STEP = "Conformance required cargo lanes (Linux)"
GATED_LINUX_IF = "runner.os == 'Linux'"
GATED_LINUX_COMMAND = (
    "python3 conformance/run_all.py --with-cargo --require-complete --completion-scope required"
)
GATED_LINUX_COMMANDS = (
    "set -euo pipefail",
    GATED_LINUX_COMMAND,
)
HARDENING_GUARD_COMMANDS = (
    "python3 scripts/ci/check-conformance-inventory-callsite.py",
    "python3 scripts/ci/test-conformance-inventory-callsite.py",
)
# Independent of the checker module. B1 keeps its own copy of this sequence.
HARDENING_STEP_COMMANDS = (
    "set -euo pipefail",
    "bash scripts/ci/test-check-assay-release-pin.sh",
    "bash scripts/ci/check-assay-release-pin.sh --published",
    "bash scripts/ci/test-check-assay-action-pin.sh",
    "bash scripts/ci/check-assay-action-pin.sh",
    "bash scripts/ci/check-assay-action-pin.sh --published",
    "bash scripts/ci/test-ci-hardening-b1.sh",
    "bash scripts/ci/test-structurizr-export-docker.sh",
    "python3 scripts/ci/check-conformance-inventory-callsite.py",
    "python3 scripts/ci/test-conformance-inventory-callsite.py",
)
# Independent of the checker module and of FINALE_RUN_SCRIPT.
FINALE_STEP_COMMANDS = (
    "set -euo pipefail",
    "python3 scripts/ci/check-ci-gate-coverage.py",
    "python3 scripts/ci/check-conformance-inventory-callsite.py",
)
HARDENING_GH_TOKEN = "GH_TOKEN: ${{ github.token }}"
HARDENING_ENV_BLOCK = (
    "        env:\n"
    f"          {HARDENING_GH_TOKEN}\n"
)


def load_checker():
    if not CHECKER_PATH.is_file():
        raise AssertionError(f"missing checker {CHECKER_PATH}")
    spec = importlib.util.spec_from_file_location(
        "conformance_inventory_callsite", CHECKER_PATH)
    if spec is None or spec.loader is None:
        raise AssertionError("checker is not loadable")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    problems_fn = getattr(module, "conformance_inventory_callsite_problems", None)
    guards_fn = getattr(module, "hardening_guard_callsite_problems", None)
    schedule_fn = getattr(module, "required_aggregator_schedule_problems", None)
    host_fn = getattr(module, "host_schedule_callsite_problems", None)
    if not callable(problems_fn):
        raise AssertionError("conformance_inventory_callsite_problems missing")
    if not callable(guards_fn):
        raise AssertionError("hardening_guard_callsite_problems missing")
    if not callable(schedule_fn):
        raise AssertionError("required_aggregator_schedule_problems missing")
    if not callable(host_fn):
        raise AssertionError("host_schedule_callsite_problems missing")
    if not callable(getattr(module, "main", None)):
        raise AssertionError("checker main missing")
    return module, problems_fn, guards_fn, schedule_fn, host_fn


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def b1_hardening_pin_rc(workflow_text: str) -> int:
    source = B1_PATH.read_text(encoding="utf-8")
    marker = 'echo "== required CI workflow actively runs both hardening contracts =="'
    start = source.index(marker)
    py_start = source.index("import re\n", start)
    py_end = source.index("\nPY\n", py_start)
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp) / "ci.yml"
        scratch.write_text(workflow_text, encoding="utf-8")
        completed = subprocess.run(
            [sys.executable, "-c", source[py_start:py_end], str(scratch)],
            capture_output=True,
            text=True,
            check=False,
        )
    return completed.returncode


def early_exit_0(text: str) -> str:
    needle = "          bash scripts/ci/test-ci-hardening-b1.sh\n"
    mutated = text.replace(needle, needle + "          exit 0\n", 1)
    if mutated == text:
        raise AssertionError("early exit 0 mutation was a no-op")
    return mutated


def swap_hardening_guards(text: str) -> str:
    first = "          python3 scripts/ci/check-conformance-inventory-callsite.py\n"
    second = "          python3 scripts/ci/test-conformance-inventory-callsite.py\n"
    if first not in text or second not in text:
        raise AssertionError("guard swap targets missing")
    mutated = text.replace(first, "__SWAP_A__\n", 1)
    mutated = mutated.replace(second, first, 1)
    mutated = mutated.replace("__SWAP_A__\n", second, 1)
    if mutated == text:
        raise AssertionError("guard swap mutation was a no-op")
    return mutated


def extra_hardening_command(text: str) -> str:
    needle = "          python3 scripts/ci/check-conformance-inventory-callsite.py\n"
    mutated = text.replace(needle, needle + "          echo extra-active\n", 1)
    if mutated == text:
        raise AssertionError("extra-command mutation was a no-op")
    return mutated


def neutralize(text: str, command: str, mode: str) -> str:
    needle = f"          {command}\n"
    if needle not in text:
        raise AssertionError(f"missing live command {command!r}")
    if mode == "remove":
        replacement = ""
    elif mode == "comment":
        replacement = f"          # {command}\n"
    elif mode == "colon":
        replacement = "          :\n"
    else:
        raise AssertionError(f"unknown neutralize mode {mode!r}")
    mutated = text.replace(needle, replacement, 1)
    if mutated == text:
        raise AssertionError(f"{mode} mutation of {command!r} was a no-op")
    return mutated


def delete_step(text: str) -> str:
    step = named_step(text, JOB, INVENTORY_STEP)
    mutated = text.replace(step, "", 1)
    if mutated == text or f"      - name: {INVENTORY_STEP}\n" in mutated:
        raise AssertionError("delete-step mutation was a no-op")
    return mutated


def rename_step(text: str) -> str:
    mutated = text.replace(
        f"      - name: {INVENTORY_STEP}\n",
        "      - name: Inventory conformance\n",
        1,
    )
    if mutated == text:
        raise AssertionError("rename mutation was a no-op")
    return mutated


def false_if(text: str) -> str:
    needle = f"      - name: {INVENTORY_STEP}\n"
    mutated = text.replace(needle, needle + "        if: false\n", 1)
    if mutated == text:
        raise AssertionError("if:false mutation was a no-op")
    return mutated


def reorder_one_command(text: str) -> str:
    first = "          python3 conformance/registry.py\n"
    second = "          python3 conformance/implementations.py\n"
    if first not in text or second not in text:
        raise AssertionError("reorder targets missing")
    mutated = text.replace(first, "__SWAP_A__\n", 1)
    mutated = mutated.replace(second, first, 1)
    mutated = mutated.replace("__SWAP_A__\n", second, 1)
    if mutated == text:
        raise AssertionError("reorder mutation was a no-op")
    return mutated


STRUCTURAL_MUTATIONS = (
    ("delete_step", delete_step),
    ("rename_step", rename_step),
    ("false_if", false_if),
    ("reorder_one_command", reorder_one_command),
)


def hardening_heading(text: str) -> str:
    heading = f"      - name: {HARDENING_STEP}\n"
    if heading not in text:
        raise AssertionError("hardening step heading missing")
    return heading


def delete_hardening_step(text: str) -> str:
    for job in (FINALE_JOB, JOB):
        try:
            step = named_step(text, job, HARDENING_STEP)
        except AssertionError:
            continue
        mutated = text.replace(step, "", 1)
        if mutated == text or f"      - name: {HARDENING_STEP}\n" in mutated:
            raise AssertionError("delete-hardening mutation was a no-op")
        return mutated
    raise AssertionError("hardening step missing")


def rename_hardening_step(text: str) -> str:
    mutated = text.replace(
        hardening_heading(text),
        "      - name: Verify CI hardening contract\n",
        1,
    )
    if mutated == text:
        raise AssertionError("rename-hardening mutation was a no-op")
    return mutated


def false_if_hardening(text: str) -> str:
    needle = hardening_heading(text)
    mutated = text.replace(needle, needle + "        if: false\n", 1)
    if mutated == text:
        raise AssertionError("if:false hardening mutation was a no-op")
    return mutated


def hostile_hardening_shell(text: str) -> str:
    needle = hardening_heading(text) + "        shell: bash\n"
    mutated = text.replace(
        needle,
        hardening_heading(text) + '        shell: bash -c "exit 0" {0}\n',
        1,
    )
    if mutated == text:
        raise AssertionError("hostile-shell mutation was a no-op")
    return mutated


def bash_env_before_hardening(text: str) -> str:
    needle = hardening_heading(text)
    writer = TRUSTED_PREFIX_WRITERS[0][1]
    mutated = text.replace(needle, writer + needle, 1)
    if mutated == text:
        raise AssertionError("BASH_ENV writer mutation was a no-op")
    return mutated


def mutate_ci_job_if(text: str, value: str | None) -> str:
    if CI_JOB_IF_BLOCK not in text:
        raise AssertionError("canonical ci job if: always() block missing")
    if value is None:
        replacement = (
            "  ci:\n"
            "    name: CI\n"
            "    runs-on: ubuntu-latest\n"
            "    timeout-minutes: 10\n"
        )
    else:
        replacement = CI_JOB_IF_BLOCK.replace("    if: always()\n", f"    if: {value}\n")
    mutated = text.replace(CI_JOB_IF_BLOCK, replacement, 1)
    if mutated == text:
        raise AssertionError(f"ci job if mutation {value!r} was a no-op")
    return mutated


def comment_between_hardening_and_finale(text: str) -> str:
    needle = finale_heading(text)
    mutated = text.replace(
        needle,
        "      # documentation-only line between hardening and finale\n" + needle,
        1,
    )
    if mutated == text:
        raise AssertionError("comment-between-steps mutation was a no-op")
    return mutated


def blank_between_hardening_and_finale(text: str) -> str:
    needle = finale_heading(text)
    mutated = text.replace(needle, "\n" + needle, 1)
    if mutated == text:
        raise AssertionError("blank-between-steps mutation was a no-op")
    return mutated


def intervening_step_between_hardening_and_finale(text: str) -> str:
    needle = finale_heading(text)
    writer = TRUSTED_PREFIX_WRITERS[0][1]
    mutated = text.replace(needle, writer + needle, 1)
    if mutated == text:
        raise AssertionError("intervening-step mutation was a no-op")
    return mutated


def host_heading(text: str) -> str:
    heading = f"      - name: {HOST_STEP}\n"
    if heading not in text:
        raise AssertionError("host scheduling step heading missing")
    return heading


def delete_host_schedule_step(text: str) -> str:
    step = named_step(text, HOST_JOB, HOST_STEP)
    mutated = text.replace(step, "", 1)
    if mutated == text or f"      - name: {HOST_STEP}\n" in mutated:
        raise AssertionError("delete-host-schedule mutation was a no-op")
    return mutated


def rename_host_schedule_step(text: str) -> str:
    mutated = text.replace(
        host_heading(text),
        "      - name: Verify required CI aggregator schedule\n",
        1,
    )
    if mutated == text:
        raise AssertionError("rename-host-schedule mutation was a no-op")
    return mutated


def false_if_host_schedule_step(text: str) -> str:
    needle = host_heading(text)
    mutated = text.replace(needle, needle + "        if: false\n", 1)
    if mutated == text:
        raise AssertionError("if:false host-schedule mutation was a no-op")
    return mutated


def neutralize_host_schedule_command(text: str, mode: str) -> str:
    command = HOST_SCHEDULE_COMMANDS[1]
    needle = f"          {command}\n"
    if needle not in text:
        raise AssertionError("host scheduling command missing")
    if mode == "remove":
        replacement = ""
    elif mode == "comment":
        replacement = f"          # {command}\n"
    else:
        raise AssertionError(f"unknown neutralize mode {mode!r}")
    mutated = text.replace(needle, replacement, 1)
    if mutated == text:
        raise AssertionError(f"{mode} mutation of host scheduling command was a no-op")
    return mutated


HOST_JOB_NAME = "    name: host-capability-check\n"


def add_host_job_key(text: str, key_line: str) -> str:
    if text.count(HOST_JOB_NAME) != 1:
        raise AssertionError("expected one host-capability-check job name")
    mutated = text.replace(HOST_JOB_NAME, HOST_JOB_NAME + f"    {key_line}\n", 1)
    if mutated == text:
        raise AssertionError(f"host job {key_line!r} mutation was a no-op")
    return mutated


HOST_JOB_MUTATIONS = (
    ("host_job_if_false", lambda text: add_host_job_key(text, "if: false")),
    ("host_job_needs", lambda text: add_host_job_key(text, "needs: [ci]")),
)


HOST_SCHEDULE_MUTATIONS = (
    ("delete_host_schedule_step", delete_host_schedule_step),
    ("rename_host_schedule_step", rename_host_schedule_step),
    ("false_if_host_schedule_step", false_if_host_schedule_step),
    ("comment_host_schedule_command",
     lambda text: neutralize_host_schedule_command(text, "comment")),
    ("remove_host_schedule_command",
     lambda text: neutralize_host_schedule_command(text, "remove")),
)


def neutralize_finale_checker(text: str) -> str:
    step = named_step(text, FINALE_JOB, FINALE_STEP)
    needle = f"          {FINALE_CHECKER}\n"
    if needle not in step:
        raise AssertionError("finale checker callsite missing")
    mutated_step = step.replace(needle, "", 1)
    mutated = text.replace(step, mutated_step, 1)
    if mutated == text:
        raise AssertionError("finale-checker neutralization was a no-op")
    return mutated


HARDENING_EXECUTION_MUTATIONS = (
    ("delete_hardening_step", delete_hardening_step),
    ("rename_hardening_step", rename_hardening_step),
    ("false_if_hardening", false_if_hardening),
    ("hostile_hardening_shell", hostile_hardening_shell),
    ("bash_env_before_hardening", bash_env_before_hardening),
)


def finale_heading(text: str) -> str:
    heading = f"      - name: {FINALE_STEP}\n"
    if heading not in text:
        raise AssertionError("finale step heading missing")
    return heading


def false_if_finale(text: str) -> str:
    needle = finale_heading(text)
    mutated = text.replace(needle, needle + "        if: false\n", 1)
    if mutated == text:
        raise AssertionError("if:false finale mutation was a no-op")
    return mutated


def hostile_finale_shell(text: str) -> str:
    heading = finale_heading(text)
    bash = heading + "        shell: bash\n"
    hostile = heading + '        shell: bash -c "exit 0" {0}\n'
    if bash in text:
        mutated = text.replace(bash, hostile, 1)
    else:
        mutated = text.replace(heading, hostile, 1)
    if mutated == text:
        raise AssertionError("hostile-finale-shell mutation was a no-op")
    return mutated


def continue_on_error_finale(text: str) -> str:
    needle = finale_heading(text)
    mutated = text.replace(needle, needle + "        continue-on-error: true\n", 1)
    if mutated == text:
        raise AssertionError("continue-on-error finale mutation was a no-op")
    return mutated


def bash_env_between_hardening_and_finale(text: str) -> str:
    needle = finale_heading(text)
    writer = TRUSTED_PREFIX_WRITERS[0][1]
    mutated = text.replace(needle, writer + needle, 1)
    if mutated == text:
        raise AssertionError("inter-step BASH_ENV writer mutation was a no-op")
    return mutated


FINALE_EXECUTION_MUTATIONS = (
    ("false_if_finale", false_if_finale),
    ("hostile_finale_shell", hostile_finale_shell),
    ("continue_on_error_finale", continue_on_error_finale),
    ("bash_env_between_hardening_and_finale", bash_env_between_hardening_and_finale),
)


def drop_hardening_gh_token(text: str) -> str:
    step = named_step(text, FINALE_JOB, HARDENING_STEP)
    if HARDENING_ENV_BLOCK not in step:
        raise AssertionError("hardening GH_TOKEN env block missing")
    mutated_step = step.replace(HARDENING_ENV_BLOCK, "", 1)
    mutated = text.replace(step, mutated_step, 1)
    if mutated == text:
        raise AssertionError("drop-GH_TOKEN mutation was a no-op")
    return mutated


def rebind_hardening_gh_token(text: str) -> str:
    step = named_step(text, FINALE_JOB, HARDENING_STEP)
    if HARDENING_GH_TOKEN not in step:
        raise AssertionError("hardening GH_TOKEN binding missing")
    mutated_step = step.replace(
        HARDENING_GH_TOKEN,
        "GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}",
        1,
    )
    mutated = text.replace(step, mutated_step, 1)
    if mutated == text:
        raise AssertionError("rebind-GH_TOKEN mutation was a no-op")
    return mutated


def run_checker_main(
        module, workflow_text: str, *args: str, host_text: str | None = None) -> int:
    original = module.CI_YML
    original_host = getattr(module, "HOST_YML", None)
    original_argv = sys.argv
    if host_text is None:
        host_text = HOST_YML.read_text(encoding="utf-8")
    with tempfile.TemporaryDirectory() as tmp:
        scratch = Path(tmp) / "ci.yml"
        scratch.write_text(workflow_text, encoding="utf-8")
        host_scratch = Path(tmp) / "host.yml"
        host_scratch.write_text(host_text, encoding="utf-8")
        module.CI_YML = scratch
        module.HOST_YML = host_scratch
        sys.argv = [str(CHECKER_PATH), *args]
        try:
            return module.main()
        finally:
            module.CI_YML = original
            if original_host is not None:
                module.HOST_YML = original_host
            sys.argv = original_argv


def run_live_finale_step(workflow_text: str, *, fail_first: bool) -> int:
    script = "\n".join(_active_run_lines(named_step(
        workflow_text, FINALE_JOB, FINALE_STEP)))
    if fail_first:
        script = (
            "python3() {\n"
            "  if [ \"$1\" = \"scripts/ci/check-ci-gate-coverage.py\" ]; then\n"
            "    return 2\n"
            "  fi\n"
            "  command python3 \"$@\"\n"
            "}\n"
            + script
        )
    completed = subprocess.run(
        ["bash", "-c", script],
        cwd=REPO,
        capture_output=True,
        text=True,
        check=False,
    )
    return completed.returncode


class ConformanceInventoryCallsite(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        cls.live = CI_YML.read_text(encoding="utf-8")
        cls.live_digest = sha256_text(cls.live)
        module, problems_fn, guards_fn, schedule_fn, host_fn = load_checker()
        cls.checker = module
        cls.problems_fn = staticmethod(problems_fn)
        cls.guards_fn = staticmethod(guards_fn)
        cls.schedule_fn = staticmethod(schedule_fn)
        cls.host_fn = staticmethod(host_fn)
        cls.host_live = HOST_YML.read_text(encoding="utf-8")
        cls.host_digest = sha256_text(cls.host_live)

    def tearDown(self) -> None:
        current = CI_YML.read_text(encoding="utf-8")
        self.assertEqual(sha256_text(current), self.live_digest)
        self.assertEqual(current, self.live)
        host = HOST_YML.read_text(encoding="utf-8")
        self.assertEqual(sha256_text(host), self.host_digest)
        self.assertEqual(host, self.host_live)

    def test_command_table_is_not_an_import_of_the_completion_scope_tuple(self) -> None:
        imported = [
            line
            for line in Path(__file__).read_text(encoding="utf-8").splitlines()
            if "import" in line and "test_completion_scope" in line
        ]
        joined = "\n".join(imported)
        self.assertNotIn("INVENTORY_" + "RUN_SCRIPT", joined)
        self.assertNotIn("REQUIRED_" + "RUN_ALL", joined)
        self.assertNotIn("GATED_" + "REQUIRED_RUN_ALL", joined)
        self.assertNotIn("GATED_" + "LINUX_RUN_SCRIPT", joined)
        self.assertNotIn("HARDENING_" + "RUN_SCRIPT", joined)
        self.assertNotIn("FINALE_" + "RUN_SCRIPT", joined)

    def test_pristine_workflow_is_green(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        self.assertEqual(self.guards_fn(self.live), [])
        self.assertEqual(self.schedule_fn(self.live), [])
        self.assertEqual(self.host_fn(self.host_live), [])
        step = named_step(self.live, FINALE_JOB, HARDENING_STEP)
        self.assertEqual(tuple(_active_run_lines(step)), HARDENING_STEP_COMMANDS)
        self.assertIn(HARDENING_ENV_BLOCK, step)
        finale = named_step(self.live, FINALE_JOB, FINALE_STEP)
        self.assertEqual(tuple(_active_run_lines(finale)), FINALE_STEP_COMMANDS)
        host = named_step(self.host_live, HOST_JOB, HOST_STEP)
        self.assertEqual(tuple(_active_run_lines(host)), HOST_SCHEDULE_COMMANDS)

    def test_live_inventory_step_matches_independent_literals(self) -> None:
        step = named_step(self.live, JOB, INVENTORY_STEP)
        self.assertEqual(tuple(_active_run_lines(step)), REQUIRED_INVENTORY_COMMANDS)

    def test_live_gated_linux_step_matches_independent_literals(self) -> None:
        step = named_step(self.live, GATED_LINUX_JOB, GATED_LINUX_STEP)
        self.assertEqual(tuple(_active_run_lines(step)), GATED_LINUX_COMMANDS)
        self.assertIn(f"        if: {GATED_LINUX_IF}\n", step)

    def test_linux_step_if_never_true_fails_the_checker(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        live_step = named_step(self.live, GATED_LINUX_JOB, GATED_LINUX_STEP)
        for replacement in (
            "        if: false\n",
            "        if: runner.os == 'Windows'\n",
            "        if: runner.os != 'Linux'\n",
        ):
            with self.subTest(replacement=replacement.strip()):
                mutated_step = live_step.replace(
                    f"        if: {GATED_LINUX_IF}\n", replacement, 1)
                mutated = self.live.replace(live_step, mutated_step, 1)
                self.assertTrue(
                    self.problems_fn(mutated),
                    f"{replacement!r} must fail the later checker")
        self.assertEqual(self.problems_fn(self.live), [])

    def test_gated_command_neutralization_fails(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        for mode in ("remove", "comment", "colon"):
            with self.subTest(mode=mode):
                problems = self.problems_fn(
                    neutralize(self.live, GATED_LINUX_COMMAND, mode))
                self.assertTrue(
                    problems,
                    f"{mode} {GATED_LINUX_COMMAND!r} must fail the later checker")
        self.assertEqual(self.problems_fn(self.live), [])

    def test_structural_mutations_fail_independently(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        for label, mutate in STRUCTURAL_MUTATIONS:
            with self.subTest(label=label):
                problems = self.problems_fn(mutate(self.live))
                self.assertTrue(problems, f"{label} must fail the later checker")
        self.assertEqual(self.problems_fn(self.live), [])

    def test_each_required_command_neutralization_fails(self) -> None:
        self.assertEqual(self.problems_fn(self.live), [])
        for command in REQUIRED_INVENTORY_COMMANDS:
            for mode in ("remove", "comment", "colon"):
                with self.subTest(command=command, mode=mode):
                    problems = self.problems_fn(
                        neutralize(self.live, command, mode))
                    self.assertTrue(
                        problems,
                        f"{mode} {command!r} must fail the later checker",
                    )
        self.assertEqual(self.problems_fn(self.live), [])

    def test_hardening_guard_call_neutralization_fails(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        for command in HARDENING_GUARD_COMMANDS:
            for mode in ("remove", "comment", "colon"):
                with self.subTest(command=command, mode=mode):
                    problems = self.guards_fn(
                        neutralize(self.live, command, mode))
                    self.assertTrue(
                        problems,
                        f"{mode} {command!r} must fail the workflow-call pin",
                    )
        self.assertEqual(self.guards_fn(self.live), [])

    def test_b1_pins_the_exact_hardening_step_sequence(self) -> None:
        text = B1_PATH.read_text(encoding="utf-8")
        marker = 'echo "== required CI workflow actively runs both hardening contracts =="'
        start = text.index(marker)
        py_start = text.index("import re\n", start)
        py_end = text.index("\nPY\n", py_start)
        block = text[py_start:py_end]
        self.assertIn("if active != list(required):", block)
        self.assertNotIn(
            "missing = [cmd for cmd in required if cmd not in active]",
            block,
        )
        for command in HARDENING_STEP_COMMANDS:
            self.assertIn(f'"{command}"', block)
        self.assertIn(HARDENING_GH_TOKEN, block)

    def test_hardening_step_sequence_mutations_fail_b1(self) -> None:
        self.assertEqual(b1_hardening_pin_rc(self.live), 0)
        mutations = (
            ("early_exit_0", early_exit_0),
            ("swap_guards", swap_hardening_guards),
            ("extra_command", extra_hardening_command),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                self.assertNotEqual(
                    b1_hardening_pin_rc(mutate(self.live)),
                    0,
                    f"{label} must fail the B1 hardening-step pin",
                )
        self.assertEqual(b1_hardening_pin_rc(self.live), 0)

    def test_hardening_execution_removal_fails_the_required_root(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        for label, mutate in HARDENING_EXECUTION_MUTATIONS:
            with self.subTest(label=label):
                problems = self.guards_fn(mutate(self.live))
                self.assertTrue(
                    problems,
                    f"{label} must fail the hardening hard-run contract",
                )
        self.assertEqual(self.guards_fn(self.live), [])

    def test_finale_ci_invokes_the_hardening_checker(self) -> None:
        step = named_step(self.live, FINALE_JOB, FINALE_STEP)
        self.assertEqual(tuple(_active_run_lines(step)), FINALE_STEP_COMMANDS)
        self.assertTrue(self.guards_fn(neutralize_finale_checker(self.live)))

    def test_finale_execution_removal_fails_the_hardening_root(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        for label, mutate in FINALE_EXECUTION_MUTATIONS:
            with self.subTest(label=label):
                problems = self.guards_fn(mutate(self.live))
                self.assertTrue(
                    problems,
                    f"{label} must fail the finale hard-run contract",
                )
        self.assertEqual(self.guards_fn(self.live), [])

    def test_hardening_gh_token_binding_mutations_fail(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        for label, mutate in (
            ("drop_hardening_gh_token", drop_hardening_gh_token),
            ("rebind_hardening_gh_token", rebind_hardening_gh_token),
        ):
            with self.subTest(label=label):
                problems = self.guards_fn(mutate(self.live))
                self.assertTrue(
                    problems,
                    f"{label} must fail the exact GH_TOKEN env pin",
                )
        self.assertEqual(self.guards_fn(self.live), [])

    def test_finale_first_checker_failure_fails_the_real_step(self) -> None:
        second = subprocess.run(
            [sys.executable, str(CHECKER_PATH)],
            cwd=REPO,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(second.returncode, 0, second.stderr)
        self.assertNotEqual(
            run_live_finale_step(self.live, fail_first=True),
            0,
            "first-checker failure must fail the live finale step",
        )

    def test_single_mutation_ownership_is_mutual(self) -> None:
        for label, mutate in FINALE_EXECUTION_MUTATIONS:
            with self.subTest(direction="finale-only", label=label):
                mutated = mutate(self.live)
                assert_hard_run_command(mutated, FINALE_JOB, HARDENING_STEP)
                self.assertTrue(
                    self.guards_fn(mutated),
                    f"{label} must fail finale ownership while hardening stays green",
                )
        for label, mutate in HARDENING_EXECUTION_MUTATIONS:
            with self.subTest(direction="hardening-only", label=label):
                mutated = mutate(self.live)
                self.assertTrue(
                    self.guards_fn(mutated),
                    f"{label} must fail hardening ownership",
                )
                finale = named_step(mutated, FINALE_JOB, FINALE_STEP)
                self.assertEqual(
                    tuple(_active_run_lines(finale)),
                    FINALE_STEP_COMMANDS,
                )

    def test_main_fails_closed_on_mutated_workflows(self) -> None:
        self.assertEqual(run_checker_main(self.checker, self.live), 0)
        mutations = (
            ("inventory_delete", delete_step),
            ("hardening_delete", delete_hardening_step),
            ("finale_if_false", false_if_finale),
            ("ci_if_false", lambda text: mutate_ci_job_if(text, "false")),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                self.assertNotEqual(
                    run_checker_main(self.checker, mutate(self.live)),
                    0,
                    f"{label} must fail checker main()",
                )
        self.assertEqual(run_checker_main(self.checker, self.live), 0)

    def test_neutralizing_main_failure_branch_is_visible(self) -> None:
        source = CHECKER_PATH.read_text(encoding="utf-8")
        needle = "    if problems:\n"
        self.assertIn(needle, source)
        self.assertNotIn("    if False and problems:\n", source)
        disabled = source.replace(needle, "    if False and problems:\n", 1)
        mutated_workflow = delete_step(self.live)
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "checker.py"
            path.write_text(disabled, encoding="utf-8")
            spec = importlib.util.spec_from_file_location(
                "disabled_inventory_callsite", path)
            if spec is None or spec.loader is None:
                raise AssertionError("disabled checker is not loadable")
            module = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(module)
            self.assertEqual(
                run_checker_main(module, mutated_workflow),
                0,
                "the reported if False and problems mutation must skip main failure",
            )
        self.assertNotEqual(
            run_checker_main(self.checker, mutated_workflow),
            0,
            "live main() must stay red on the same mutated workflow",
        )

    def test_ci_aggregator_if_mutations_fail_the_host_root(self) -> None:
        self.assertEqual(self.schedule_fn(self.live), [])
        self.assertEqual(
            run_checker_main(
                self.checker, self.live, "--required-aggregator-schedule"),
            0,
        )
        mutations = (
            ("if_false", lambda text: mutate_ci_job_if(text, "false")),
            ("if_missing", lambda text: mutate_ci_job_if(text, None)),
            ("if_success", lambda text: mutate_ci_job_if(text, "success()")),
        )
        for label, mutate in mutations:
            with self.subTest(label=label):
                mutated = mutate(self.live)
                self.assertEqual(self.problems_fn(mutated), [])
                self.assertTrue(
                    self.schedule_fn(mutated),
                    f"{label} must fail the host-root schedule pin",
                )
                self.assertNotEqual(
                    run_checker_main(
                        self.checker, mutated, "--required-aggregator-schedule"),
                    0,
                    f"{label} must fail checker --required-aggregator-schedule",
                )
        self.assertEqual(self.schedule_fn(self.live), [])

    def test_host_schedule_invocation_mutations_fail(self) -> None:
        self.assertEqual(self.host_fn(self.host_live), [])
        self.assertEqual(run_checker_main(self.checker, self.live), 0)
        for label, mutate in HOST_SCHEDULE_MUTATIONS:
            with self.subTest(label=label):
                mutated = mutate(self.host_live)
                self.assertTrue(
                    self.host_fn(mutated),
                    f"{label} must fail the host scheduling callsite pin",
                )
                self.assertNotEqual(
                    run_checker_main(self.checker, self.live, host_text=mutated),
                    0,
                    f"{label} must fail checker main()",
                )
        self.assertEqual(self.host_fn(self.host_live), [])
        self.assertEqual(run_checker_main(self.checker, self.live), 0)

    def test_parent_host_job_if_false_after_name_fails_the_ci_root(self) -> None:
        mutated = add_host_job_key(self.host_live, "if: false")
        self.assertIn(
            "    name: host-capability-check\n    if: false\n", mutated)
        self.assertTrue(
            self.host_fn(mutated),
            "job-level if: false after the host job name must fail the CI-root pin",
        )
        self.assertNotEqual(
            run_checker_main(self.checker, self.live, host_text=mutated),
            0,
            "job-level if: false must fail checker main()",
        )
        self.assertEqual(self.host_fn(self.host_live), [])

    def test_host_job_if_or_needs_fails_the_ci_root(self) -> None:
        self.assertEqual(self.host_fn(self.host_live), [])
        self.assertEqual(run_checker_main(self.checker, self.live), 0)
        for label, mutate in HOST_JOB_MUTATIONS:
            with self.subTest(label=label):
                mutated = mutate(self.host_live)
                self.assertTrue(
                    self.host_fn(mutated),
                    f"{label} must fail the CI-root host-job pin",
                )
                self.assertNotEqual(
                    run_checker_main(self.checker, self.live, host_text=mutated),
                    0,
                    f"{label} must fail checker main()",
                )
        self.assertEqual(self.host_fn(self.host_live), [])
        self.assertEqual(run_checker_main(self.checker, self.live), 0)

    def test_blank_or_comment_between_hardening_and_finale_stays_green(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        for label, mutate in (
            ("blank", blank_between_hardening_and_finale),
            ("comment", comment_between_hardening_and_finale),
        ):
            with self.subTest(label=label):
                self.assertEqual(
                    self.guards_fn(mutate(self.live)),
                    [],
                    f"{label} lines must not break hardening/finale successor identity",
                )
        self.assertEqual(self.guards_fn(self.live), [])

    def test_intervening_executable_step_still_fails_successor(self) -> None:
        self.assertEqual(self.guards_fn(self.live), [])
        self.assertTrue(
            self.guards_fn(intervening_step_between_hardening_and_finale(self.live)),
            "an intervening executable step must still fail successor identity",
        )
        self.assertEqual(self.guards_fn(self.live), [])


if __name__ == "__main__":
    unittest.main()
