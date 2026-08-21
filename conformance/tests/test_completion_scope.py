#!/usr/bin/env python3
"""Explicit --completion-scope contract. Does not redefine global complete.

    python3 -W error::ResourceWarning conformance/tests/test_completion_scope.py

Default --require-complete still exits 3 when global complete is false.
--completion-scope required is only valid with --require-complete and exits 3
on required_complete, not by mapping 3 to 0.
"""

from __future__ import annotations

import importlib.util
import json
import re
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import registry  # noqa: E402
import run_all  # noqa: E402

REPO = Path(__file__).resolve().parents[2]
CI_YML = REPO / ".github/workflows/ci.yml"
STANDALONE = REPO / ".github/workflows/conformance-inventory.yml"
REQUIRED_RUN_ALL = (
    "python3 conformance/run_all.py --require-complete --completion-scope required"
)
CONFORMANCE_YML = REPO / ".github/workflows/privileged-mcp-action-conformance.yml"
COMBINED_UNITTEST = (
    "          python3 -m unittest \\\n"
    "            conformance/privileged-mcp-action-v0/tests/test_activation_kit.py \\\n"
    "            conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py"
)
INVENTORY_RUN_SCRIPT = (
    "set -euo pipefail",
    "python3 conformance/registry.py",
    "python3 conformance/implementations.py",
    "python3 -W error::ResourceWarning conformance/tests/test_implementations.py",
    "python3 -W error::ResourceWarning conformance/tests/test_pma_v0_registration.py",
    "python3 -W error::ResourceWarning conformance/tests/test_registry.py",
    "python3 -W error::ResourceWarning conformance/tests/test_run_all.py",
    "python3 -W error::ResourceWarning conformance/tests/test_completion_scope.py",
    REQUIRED_RUN_ALL,
)
ACTIVATION_KIT_RUN_SCRIPT = (
    "set -euo pipefail",
    *(line.strip() for line in COMBINED_UNITTEST.splitlines()),
)
HARD_RUN_CONTRACTS = {
    ("scope", "Conformance inventory"): (
        frozenset({"name", "runs-on", "timeout-minutes", "permissions",
                   "outputs", "steps"}),
        frozenset({"name", "shell", "run"}),
        INVENTORY_RUN_SCRIPT,
    ),
    ("activation-kit", "Run activation-kit contract tests"): (
        frozenset({"runs-on", "steps"}),
        frozenset({"name", "shell", "run"}),
        ACTIVATION_KIT_RUN_SCRIPT,
    ),
}
ACTIVATION_KIT = (
    REPO / "conformance/privileged-mcp-action-v0/tests/test_activation_kit.py"
)


def _indentation_bounded_block(
        lines: list[str], start: int, block_indent: int) -> str:
    end = start + 1
    while end < len(lines):
        line = lines[end]
        if line.strip():
            indent = len(line) - len(line.lstrip(" "))
            if indent <= block_indent:
                break
        end += 1
    return "".join(lines[start:end])


def named_job(text: str, name: str) -> str:
    lines = text.splitlines(keepends=True)
    marker = re.compile(rf"^  {re.escape(name)}:\s*$")
    matches = [
        (i, marker.fullmatch(line.rstrip("\r\n")))
        for i, line in enumerate(lines)
    ]
    matches = [(i, match) for i, match in matches if match is not None]
    if len(matches) != 1:
        raise AssertionError(f"expected one {name!r} job, found {len(matches)}")
    return _indentation_bounded_block(lines, matches[0][0], 2)


def named_step(text: str, job_name: str, step_name: str) -> str:
    lines = named_job(text, job_name).splitlines(keepends=True)
    marker = re.compile(rf"^(?P<indent> +)- name: {re.escape(step_name)}\s*$")
    matches = [
        (i, marker.fullmatch(line.rstrip("\r\n")))
        for i, line in enumerate(lines)
    ]
    matches = [(i, match) for i, match in matches if match is not None]
    if len(matches) != 1:
        raise AssertionError(
            f"expected one {step_name!r} step in {job_name!r}, found {len(matches)}")
    start, match = matches[0]
    assert match is not None
    return _indentation_bounded_block(
        lines, start, len(match.group("indent")))


def _inventory_step(text: str) -> str:
    return named_step(text, "scope", "Conformance inventory")


def _active_run_lines(step: str) -> list[str]:
    raw_lines = step.splitlines()
    if not raw_lines:
        return []
    step_indent = len(raw_lines[0]) - len(raw_lines[0].lstrip(" "))
    run_indent = step_indent + 2
    run_indexes = [
        i for i, raw in enumerate(raw_lines)
        if raw == " " * run_indent + "run: |"
    ]
    if len(run_indexes) != 1:
        return []
    lines: list[str] = []
    for raw in raw_lines[run_indexes[0] + 1:]:
        stripped = raw.strip()
        indent = len(raw) - len(raw.lstrip(" "))
        if stripped and indent <= run_indent:
            break
        if not stripped or stripped.startswith("#"):
            continue
        lines.append(stripped)
    return lines


DIRECT_ENTRY_RE = re.compile(
    r"^(?:(?P<plain>[A-Za-z0-9_-]+)|'(?P<single>[A-Za-z0-9_-]+)'|"
    r'"(?P<double>[A-Za-z0-9_-]+)")\s*:(?P<value>.*)$')


def _direct_mapping(block: str) -> dict[str, str]:
    lines = block.splitlines()
    if not lines:
        raise AssertionError("empty mapping block")
    block_indent = len(lines[0]) - len(lines[0].lstrip(" "))
    sequence_item = lines[0][block_indent:].startswith("- ")
    direct_indent = block_indent + 2 if sequence_item else None
    if direct_indent is None:
        for line in lines[1:]:
            if not line.strip() or line.lstrip().startswith("#"):
                continue
            direct_indent = len(line) - len(line.lstrip(" "))
            if direct_indent <= block_indent:
                raise AssertionError("mapping block has no indented entry")
            break
    if direct_indent is None:
        raise AssertionError("mapping block has no direct entries")

    entries: dict[str, str] = {}
    for index, line in enumerate(lines):
        if not line.strip() or line.lstrip().startswith("#"):
            continue
        indent = len(line) - len(line.lstrip(" "))
        if index == 0 and sequence_item:
            direct = line[indent + 2:]
        elif indent == direct_indent:
            direct = line[direct_indent:]
        elif block_indent < indent < direct_indent:
            raise AssertionError("inconsistent direct mapping indentation")
        else:
            continue
        match = DIRECT_ENTRY_RE.fullmatch(direct)
        if match is None:
            raise AssertionError(f"unrecognized direct mapping key syntax: {direct!r}")
        key = next(match.group(name) for name in ("plain", "single", "double")
                   if match.group(name) is not None)
        if key in entries:
            raise AssertionError(f"duplicate direct mapping key: {key}")
        entries[key] = match.group("value")
    return entries


def _direct_scalar(raw: str) -> str:
    value = raw.strip()
    if re.fullmatch(r"[A-Za-z0-9_.-]+", value):
        return value
    if re.fullmatch(r"'(?:[^']|'')*'", value):
        return value[1:-1].replace("''", "'")
    if value.startswith('"') and value.endswith('"'):
        try:
            parsed = json.loads(value)
        except json.JSONDecodeError as error:
            raise AssertionError(
                f"unsupported direct scalar syntax: {raw!r}") from error
        if isinstance(parsed, str):
            return parsed
    raise AssertionError(f"unsupported direct scalar syntax: {raw!r}")


def assert_hard_run_command(
        text: str, job_name: str, step_name: str) -> str:
    contract = HARD_RUN_CONTRACTS.get((job_name, step_name))
    if contract is None:
        raise AssertionError(f"no hard-run contract for {job_name}/{step_name}")
    allowed_job_keys, allowed_step_keys, expected_script = contract

    job = named_job(text, job_name)
    actual_job_keys = frozenset(_direct_mapping(job))
    if actual_job_keys != allowed_job_keys:
        raise AssertionError(
            f"{job_name} job direct keys differ: "
            f"expected {sorted(allowed_job_keys)}, got {sorted(actual_job_keys)}")

    step = named_step(text, job_name, step_name)
    step_entries = _direct_mapping(step)
    actual_step_keys = frozenset(step_entries)
    if actual_step_keys != allowed_step_keys:
        raise AssertionError(
            f"{step_name} direct keys differ: "
            f"expected {sorted(allowed_step_keys)}, got {sorted(actual_step_keys)}")
    if _direct_scalar(step_entries["shell"]) != "bash":
        raise AssertionError(f"{step_name} must use shell: bash")

    active = tuple(_active_run_lines(step))
    if active != expected_script:
        raise AssertionError(f"{step_name} run script differs from the canonical script")
    return step


def _load_activation_kit():
    spec = importlib.util.spec_from_file_location(
        "pma_activation_kit_required_ci", ACTIVATION_KIT)
    if spec is None or spec.loader is None:
        raise AssertionError("activation kit is not loadable")
    mod = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(mod)
    return mod


def assert_oci_candidate_checker(mod) -> None:
    checker = getattr(mod, "oci_candidate_workflow_problems", None)
    if not callable(checker):
        raise AssertionError("oci_candidate_workflow_problems missing")
    cls = getattr(mod, "PrivilegedMcpActionOciCandidateWorkflowContract", None)
    if cls is None:
        raise AssertionError("PrivilegedMcpActionOciCandidateWorkflowContract missing")
    if not hasattr(cls, "test_mutations_fail_independently"):
        raise AssertionError("standalone mutation matrix missing")
    workflow = getattr(mod, "OCI_CANDIDATE_WORKFLOW", None)
    if workflow is None:
        raise AssertionError("OCI_CANDIDATE_WORKFLOW missing")
    text = workflow.read_text(encoding="utf-8")
    problems = checker(text)
    if problems:
        raise AssertionError(f"committed workflow problems: {problems}")
    executor = getattr(mod, "OCI_EXECUTOR", None)
    if not executor:
        raise AssertionError("OCI_EXECUTOR missing")
    mutations = (
        ("        if: success()\n", "        if: always()\n", "upload not gated on success"),
        ("gh attestation verify", "", "missing gh attestation verify"),
        (f"python3 {executor}", "python3 -c 'pass'", "missing canonical executor"),
    )
    for needle, replacement, expected in mutations:
        mutated = text.replace(needle, replacement, 1)
        if mutated == text:
            raise AssertionError(f"mutation needle missing: {needle!r}")
        got = checker(mutated)
        if not any(expected in problem for problem in got):
            raise AssertionError(f"expected {expected!r} in {got}")
    if checker(text.replace("no-such-token", "no-such-token")):
        raise AssertionError("no-op control was not green")


def assert_combined_unittest(text: str) -> None:
    assert_hard_run_command(
        text, "activation-kit", "Run activation-kit contract tests")


POLICIES = {
    "mcp-jsonrpc-id-conformance": "required",
    "rfc8785-canonicalization": "optional",
    "mcp-era-parity-v0": "optional",
    "privileged-mcp-action-v0": "external-candidate",
    "privileged-mcp-action-v1": "external-candidate",
    "observed-effect-v0": "external-candidate",
}


class RegistryPolicies(unittest.TestCase):
    def test_measured_required_optional_external_candidate(self):
        got = {s["id"]: s["policy"] for s in registry.load_suites()}
        self.assertEqual(got, POLICIES)


class ScopeContract(unittest.TestCase):
    def test_summarize_complete_is_still_executed_equals_declared(self):
        results = [
            {"id": "ran", "grade": run_all.PROVED, "policy": "required"},
            {"id": "skip", "grade": run_all.NOT_SELECTED, "policy": "optional"},
        ]
        s = run_all.summarize(results)
        self.assertEqual(s["declared"], 2)
        self.assertEqual(s["executed"], 1)
        self.assertIs(s["complete"], False)

    def test_required_counts_do_not_change_global_complete(self):
        results = [
            {"id": "ran", "grade": run_all.PROVED, "policy": "required"},
            {"id": "skip", "grade": run_all.NOT_SELECTED, "policy": "optional"},
            {"id": "need", "grade": run_all.NEEDS_CANDIDATE, "policy": "external-candidate"},
        ]
        scoped = run_all.required_counts(results)
        self.assertEqual(scoped["required_declared"], 1)
        self.assertEqual(scoped["required_executed"], 1)
        self.assertIs(scoped["required_complete"], True)
        self.assertIs(run_all.summarize(results)["complete"], False)

    def test_empty_required_set_is_not_required_complete(self):
        results = [{"id": "need", "grade": run_all.NEEDS_CANDIDATE, "policy": "external-candidate"}]
        scoped = run_all.required_counts(results)
        self.assertEqual(scoped["required_declared"], 0)
        self.assertIs(scoped["required_complete"], False)

    def test_default_require_complete_exits_3_on_global_incomplete(self):
        results = [
            {"id": "ran", "grade": run_all.PROVED, "policy": "required"},
            {"id": "skip", "grade": run_all.NOT_SELECTED, "policy": "optional"},
        ]
        self.assertEqual(run_all.exit_status(results, require_complete=True), 3)
        self.assertEqual(
            run_all.exit_status(results, require_complete=True, completion_scope="all"), 3)

    def test_required_scope_exits_0_when_required_subset_executed(self):
        results = [
            {"id": "ran", "grade": run_all.PROVED, "policy": "required"},
            {"id": "skip", "grade": run_all.NOT_SELECTED, "policy": "optional"},
            {"id": "need", "grade": run_all.NEEDS_CANDIDATE, "policy": "external-candidate"},
        ]
        self.assertEqual(
            run_all.exit_status(
                results, require_complete=True, completion_scope="required"), 0)
        self.assertEqual(run_all.exit_status(results, require_complete=True), 3)

    def test_required_scope_exits_3_when_required_subset_did_not_run(self):
        results = [
            {"id": "need", "grade": run_all.NOT_SELECTED, "policy": "required"},
            {"id": "ext", "grade": run_all.EXTERNAL, "policy": "external-candidate"},
        ]
        self.assertIs(run_all.required_counts(results)["required_complete"], False)
        self.assertEqual(
            run_all.exit_status(
                results, require_complete=True, completion_scope="required"), 3)

    def test_false_still_outranks_required_scope_incomplete(self):
        results = [
            {"id": "bad", "grade": run_all.FALSE, "policy": "required"},
            {"id": "skip", "grade": run_all.NOT_SELECTED, "policy": "optional"},
        ]
        self.assertEqual(
            run_all.exit_status(
                results, require_complete=True, completion_scope="required"), 1)


class ScopeThroughMain(unittest.TestCase):
    def _main(self, suites, argv):
        orig_suites, orig_argv = run_all.SUITES, sys.argv
        run_all.SUITES = suites
        sys.argv = argv
        try:
            return run_all.main()
        finally:
            run_all.SUITES = orig_suites
            sys.argv = orig_argv

    def _suite(self, ident, kind, policy, runner=None):
        row = {"id": ident, "kind": kind, "policy": policy, "vectors": 1,
               "maturity": "test", "path": "does/not/exist", "note": "fixture"}
        if runner is not None:
            row["runner"] = runner
        if kind == "cargo":
            row.update(crate="c", cargo_target_flag="--lib", cargo_target="t")
        return row

    def test_main_default_scope_still_exits_3_when_global_incomplete(self):
        suites = [
            self._suite("ok", "stdlib", "required", lambda _s: (run_all.PROVED, "ok")),
            self._suite("skip", "cargo", "optional"),
        ]
        self.assertEqual(self._main(suites, ["run_all.py", "--require-complete"]), 3)

    def test_main_required_scope_exits_0_when_required_ran(self):
        suites = [
            self._suite("ok", "stdlib", "required", lambda _s: (run_all.PROVED, "ok")),
            self._suite("skip", "cargo", "optional"),
        ]
        code = self._main(
            suites, ["run_all.py", "--require-complete", "--completion-scope", "required"])
        self.assertEqual(code, 0)

    def test_completion_scope_without_require_complete_is_an_error(self):
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--completion-scope", "required"],
            capture_output=True, text=True, timeout=60)
        self.assertNotEqual(p.returncode, 0)
        self.assertNotEqual(p.returncode, 3)


class RealTreeScopes(unittest.TestCase):
    def test_default_require_complete_still_exits_3_and_complete_is_false(self):
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json", "--require-complete"],
            capture_output=True, text=True, timeout=300)
        self.assertEqual(p.returncode, 3)
        d = json.loads(p.stdout)
        self.assertIs(d["complete"], False)
        self.assertEqual(d["declared"], 6)
        self.assertEqual(d["completion_scope"], "all")
        self.assertEqual(d["required_declared"], 1)
        self.assertEqual(d["required_executed"], 1)
        self.assertIs(d["required_complete"], True)
        self.assertEqual(len(d["suites"]), 6)

    def test_required_scope_is_green_on_the_real_tree_without_calling_that_complete(self):
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json",
             "--require-complete", "--completion-scope", "required"],
            capture_output=True, text=True, timeout=300)
        self.assertEqual(p.returncode, 0)
        d = json.loads(p.stdout)
        self.assertIs(d["complete"], False)
        self.assertEqual(d["completion_scope"], "required")
        self.assertIs(d["required_complete"], True)
        self.assertEqual(d["declared"], 6)
        self.assertLess(d["executed"], d["declared"])
        grades = {s["id"]: s["grade"] for s in d["suites"]}
        self.assertEqual(grades["mcp-jsonrpc-id-conformance"], run_all.PROVED)
        self.assertEqual(grades["privileged-mcp-action-v0"], run_all.NEEDS_CANDIDATE)
        self.assertEqual(grades["observed-effect-v0"], run_all.EXTERNAL)

    def test_plain_json_always_reports_scope_fields(self):
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json"],
            capture_output=True, text=True, timeout=300)
        d = json.loads(p.stdout)
        self.assertEqual(d["completion_scope"], "all")
        self.assertEqual(d["required_declared"], 1)
        self.assertIn("required_executed", d)
        self.assertIn("required_complete", d)
        self.assertIs(d["complete"], False)


class ProductCallsite(unittest.TestCase):
    def test_no_separate_inventory_workflow(self):
        self.assertFalse(STANDALONE.exists(), STANDALONE)

    def test_direct_key_parser_ignores_nested_soft_failure_names(self):
        block = (
            "  scope:\n"
            "    \"runs-on\": ubuntu-latest\n"
            "    outputs:\n"
            "      if: nested-output\n"
            "      continue-on-error: nested-output\n"
            "    'steps': []\n"
        )
        self.assertEqual(
            frozenset(_direct_mapping(block)),
            frozenset({"runs-on", "outputs", "steps"}),
        )

    def test_direct_key_parser_fails_closed_on_unrecognized_syntax(self):
        block = "  scope:\n    ? [if]\n    : false\n"
        with self.assertRaises(AssertionError):
            _direct_mapping(block)

    def test_scope_job_invokes_both_flags_and_does_not_map_3_to_0(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = assert_hard_run_command(
            text, "scope", "Conformance inventory")
        lines = _active_run_lines(step)
        self.assertEqual(tuple(lines), INVENTORY_RUN_SCRIPT)

    def test_commented_or_colon_run_all_line_is_not_an_active_callsite(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        commented = step.replace(REQUIRED_RUN_ALL, "# " + REQUIRED_RUN_ALL)
        self.assertNotIn(REQUIRED_RUN_ALL, _active_run_lines(commented))
        replaced = step.replace(REQUIRED_RUN_ALL, ":")
        self.assertNotIn(REQUIRED_RUN_ALL, _active_run_lines(replaced))
        self.assertIn(REQUIRED_RUN_ALL, _active_run_lines(step))

    def test_deleting_the_scope_job_callsite_fails(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "      - name: Conformance inventory\n",
            "      - name: Deleted conformance inventory\n",
            1,
        )
        with self.assertRaises(AssertionError):
            _inventory_step(mutated)

    def test_deleting_require_complete_callsite_fails(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        self.assertIn("--require-complete", step)
        self.assertNotIn("--require-complete", step.replace("--require-complete", ""))

    def test_deleting_completion_scope_required_callsite_fails(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        self.assertIn("--completion-scope required", step)
        self.assertNotIn("--completion-scope required",
                         step.replace("--completion-scope required", ""))

    def test_registry_py_does_not_neutralize_exit_3(self):
        source = Path(registry.__file__).read_text(encoding="utf-8")
        self.assertNotRegex(source, r"returncode in \(0, 3\)")
        self.assertNotIn("if completed.returncode in (0, 3)", source)

    def test_deleting_registry_suite_callsite_fails(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        self.assertTrue(any("conformance/tests/test_registry.py" in ln
                            for ln in _active_run_lines(step)))
        mutated = step.replace("conformance/tests/test_registry.py", "")
        self.assertFalse(any("conformance/tests/test_registry.py" in ln
                             for ln in _active_run_lines(mutated)))

    def test_conformance_combined_unittest_is_a_hard_callsite(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        assert_combined_unittest(text)
        assert_combined_unittest(text.replace("no-such-token", "no-such-token"))
        name = "      - name: Run activation-kit contract tests\n"
        mutations = (
            text.replace(COMBINED_UNITTEST, "          true"),
            text.replace("          python3 -m unittest \\",
                         "          # python3 -m unittest \\"),
            text.replace(COMBINED_UNITTEST, "          :"),
            text.replace("test_activation_kit.py", ""),
            text.replace("test_oci_candidate_executor.py", ""),
            text.replace(COMBINED_UNITTEST, COMBINED_UNITTEST + " || true"),
            text.replace(COMBINED_UNITTEST, COMBINED_UNITTEST + " || :"),
            text.replace("          set -euo pipefail", "          set +e", 1),
            text.replace(name, name + "        continue-on-error: true\n"),
        )
        for index, mutated in enumerate(mutations):
            with self.subTest(index=index):
                with self.assertRaises(AssertionError):
                    assert_combined_unittest(mutated)

    def test_activation_kit_step_uses_explicit_bash(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        step = named_step(
            text,
            "activation-kit",
            "Run activation-kit contract tests",
        )
        self.assertIn("        shell: bash\n", step)
        mutations = (
            step.replace(
                "        shell: bash\n",
                "        shell: bash -c 'true' -- {0}\n",
                1,
            ),
            step.replace("        shell: bash\n", "", 1),
        )
        for index, mutated_step in enumerate(mutations):
            with self.subTest(index=index):
                self.assertNotEqual(mutated_step, step)
                with self.assertRaises(AssertionError):
                    assert_combined_unittest(text.replace(step, mutated_step, 1))

    def test_explicit_activation_shell_overrides_workflow_default(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "permissions:\n",
            "defaults:\n"
            "  run:\n"
            "    shell: bash -c 'true' -- {0}\n\n"
            "permissions:\n",
            1,
        )
        self.assertNotEqual(mutated, text)
        assert_combined_unittest(mutated)

    def test_wider_indented_conditional_activation_job_fails(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        job = named_job(text, "activation-kit")
        lines = job.splitlines(keepends=True)
        widened = (
            lines[0]
            + "      if: ${{ github.event_name == 'disabled' }}\n"
            + "".join("  " + line if line.strip() else line for line in lines[1:])
        )
        with self.assertRaises(AssertionError):
            assert_combined_unittest(text.replace(job, widened, 1))

    def test_conditional_activation_kit_step_fails_the_hard_callsite(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "      - name: Run activation-kit contract tests\n",
            "      - name: Run activation-kit contract tests\n"
            "        if: ${{ github.event_name == 'disabled' }}\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_combined_unittest(mutated)

    def test_conditional_activation_kit_job_fails_the_hard_callsite(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "  activation-kit:\n",
            "  activation-kit:\n"
            "    if: ${{ github.event_name == 'disabled' }}\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_combined_unittest(mutated)

    def test_softened_activation_kit_job_fails_the_hard_callsite(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutated = text.replace(
            "  activation-kit:\n",
            "  activation-kit:\n    continue-on-error: true\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_combined_unittest(mutated)

    def test_quoted_activation_kit_job_and_step_keys_fail(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        job = "  activation-kit:\n"
        step = "      - name: Run activation-kit contract tests\n"
        mutations = (
            ("job double if", job,
             job + "    \"if\": ${{ github.event_name == 'disabled' }}\n"),
            ("job single continue", job,
             job + "    'continue-on-error': true\n"),
            ("step double if", step,
             step + "        \"if\": ${{ github.event_name == 'disabled' }}\n"),
            ("step single continue", step,
             step + "        'continue-on-error': true\n"),
        )
        for label, needle, replacement in mutations:
            with self.subTest(label=label):
                with self.assertRaises(AssertionError):
                    assert_combined_unittest(text.replace(needle, replacement, 1))

    def test_shell_neutralizers_fail_the_activation_kit_guard(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutations = (
            text.replace(
                "          set -euo pipefail\n",
                "          set -euo pipefail\n          set +o errexit\n",
                1,
            ).replace(COMBINED_UNITTEST, COMBINED_UNITTEST + "\n          true", 1),
            text.replace(
                "          set -euo pipefail\n",
                "          set -euo pipefail\n          python3() { :; }\n",
                1,
            ),
        )
        for index, mutated in enumerate(mutations):
            with self.subTest(index=index):
                with self.assertRaises(AssertionError):
                    assert_combined_unittest(mutated)

    def test_relocated_activation_kit_command_fails_the_hard_callsite(self):
        text = CONFORMANCE_YML.read_text(encoding="utf-8")
        mutated = text.replace(COMBINED_UNITTEST, "          :", 1).replace(
            "      - name: Validate public JSON documents\n",
            "      - name: Decorative activation-kit note\n"
            "        run: |\n"
            f"{COMBINED_UNITTEST}\n\n"
            "      - name: Validate public JSON documents\n",
            1,
        )
        with self.assertRaises(AssertionError):
            assert_combined_unittest(mutated)

    def test_inventory_step_moved_to_conditional_job_fails(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = named_step(text, "scope", "Conformance inventory")
        mutated = text.replace(step, "", 1)
        job_start = mutated.index("  ebpf-smoke-ubuntu:\n")
        insert_at = mutated.index("    steps:\n", job_start) + len("    steps:\n")
        mutated = mutated[:insert_at] + step + mutated[insert_at:]
        with self.assertRaises(AssertionError):
            assert_hard_run_command(
                mutated, "scope", "Conformance inventory")

    def test_required_ci_invokes_oci_candidate_checker(self):
        mod = _load_activation_kit()
        assert_oci_candidate_checker(mod)
        cls = mod.PrivilegedMcpActionOciCandidateWorkflowContract
        drops = (
            (mod, "oci_candidate_workflow_problems"),
            (mod, "PrivilegedMcpActionOciCandidateWorkflowContract"),
            (cls, "test_mutations_fail_independently"),
        )
        for owner, name in drops:
            saved = getattr(owner, name)
            delattr(owner, name)
            try:
                with self.assertRaises(AssertionError):
                    assert_oci_candidate_checker(mod)
            finally:
                setattr(owner, name, saved)


if __name__ == "__main__":
    unittest.main(verbosity=2)
