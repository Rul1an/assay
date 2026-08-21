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
INVENTORY_STEP_RE = re.compile(r"(?ms)^      - name: Conformance inventory\n(?:        .+\n)+")
JOB_KEY_RE = re.compile(r"^  [A-Za-z0-9_][A-Za-z0-9_-]*:")
REQUIRED_RUN_ALL = (
    "python3 conformance/run_all.py --require-complete --completion-scope required"
)
CONFORMANCE_YML = REPO / ".github/workflows/privileged-mcp-action-conformance.yml"
KIT_STEP_RE = re.compile(
    r"(?ms)^      - name: Run activation-kit contract tests\n(?:        .+\n)+")
COMBINED_UNITTEST = (
    "          python3 -m unittest \\\n"
    "            conformance/privileged-mcp-action-v0/tests/test_activation_kit.py \\\n"
    "            conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py"
)
ACTIVATION_KIT = (
    REPO / "conformance/privileged-mcp-action-v0/tests/test_activation_kit.py"
)


def _scope_job(text: str) -> str:
    lines = text.splitlines(keepends=True)
    start = next((i for i, line in enumerate(lines) if line.startswith("  scope:")), None)
    if start is None:
        raise AssertionError("ci.yml missing scope job")
    end = start + 1
    while end < len(lines) and not JOB_KEY_RE.match(lines[end]):
        end += 1
    return "".join(lines[start:end])


def _inventory_step(text: str) -> str:
    step = INVENTORY_STEP_RE.search(_scope_job(text))
    if step is None:
        raise AssertionError("scope job missing Conformance inventory step")
    return step.group(0)


def _active_run_lines(step: str) -> list[str]:
    lines: list[str] = []
    in_run = False
    for raw in step.splitlines():
        if raw.strip() == "run: |":
            in_run = True
            continue
        if not in_run:
            continue
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        lines.append(stripped)
    return lines


def _kit_step(text: str) -> str:
    step = KIT_STEP_RE.search(text)
    if step is None:
        raise AssertionError("conformance workflow missing kit contract step")
    return step.group(0)


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


def assert_combined_unittest(step: str) -> None:
    if COMBINED_UNITTEST not in step:
        raise AssertionError("combined unittest command missing")
    lines = _active_run_lines(step)
    joined = "\n".join(lines)
    if "python3 -m unittest" not in joined:
        raise AssertionError("combined unittest is not active")
    if "test_activation_kit.py" not in joined:
        raise AssertionError("activation kit is not active")
    if "test_oci_candidate_executor.py" not in joined:
        raise AssertionError("executor kit is not active")
    if "|| true" in step or "|| :" in step or "continue-on-error" in step:
        raise AssertionError("combined unittest is neutralized")
    if any(ln == "set +e" or ln.startswith("set +e ") for ln in lines):
        raise AssertionError("kit contract step disables -e")


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

    def test_scope_job_invokes_both_flags_and_does_not_map_3_to_0(self):
        text = CI_YML.read_text(encoding="utf-8")
        step = _inventory_step(text)
        lines = _active_run_lines(step)
        self.assertEqual(lines[0], "set -euo pipefail")
        self.assertIn("python3 conformance/registry.py", lines)
        self.assertTrue(any("conformance/tests/test_registry.py" in ln for ln in lines))
        self.assertTrue(any("conformance/tests/test_run_all.py" in ln for ln in lines))
        self.assertTrue(any("conformance/tests/test_completion_scope.py" in ln for ln in lines))
        self.assertIn(REQUIRED_RUN_ALL, lines)
        self.assertEqual(lines.count(REQUIRED_RUN_ALL), 1)
        self.assertFalse(any("||" in ln for ln in lines if REQUIRED_RUN_ALL in ln))
        self.assertFalse(any(ln == "set +e" or ln.startswith("set +e ") for ln in lines))
        self.assertNotIn("|| true", step)
        self.assertNotIn("|| :", step)
        self.assertNotIn("continue-on-error", step)
        self.assertNotRegex(step, r"eq 3|returncode in \(0, 3\)")

    def test_commented_or_colon_run_all_line_is_not_an_active_callsite(self):
        step = _inventory_step(CI_YML.read_text(encoding="utf-8"))
        commented = step.replace(REQUIRED_RUN_ALL, "# " + REQUIRED_RUN_ALL)
        self.assertNotIn(REQUIRED_RUN_ALL, _active_run_lines(commented))
        replaced = step.replace(REQUIRED_RUN_ALL, ":")
        self.assertNotIn(REQUIRED_RUN_ALL, _active_run_lines(replaced))
        self.assertIn(REQUIRED_RUN_ALL, _active_run_lines(step))

    def test_deleting_the_scope_job_callsite_fails(self):
        text = CI_YML.read_text(encoding="utf-8")
        mutated = INVENTORY_STEP_RE.sub("", text)
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
        step = _kit_step(CONFORMANCE_YML.read_text(encoding="utf-8"))
        assert_combined_unittest(step)
        assert_combined_unittest(step.replace("no-such-token", "no-such-token"))
        name = "      - name: Run activation-kit contract tests\n"
        mutations = (
            step.replace(COMBINED_UNITTEST, "          true"),
            step.replace("          python3 -m unittest \\",
                         "          # python3 -m unittest \\"),
            step.replace(COMBINED_UNITTEST, "          :"),
            step.replace("test_activation_kit.py", ""),
            step.replace("test_oci_candidate_executor.py", ""),
            step.replace(COMBINED_UNITTEST, COMBINED_UNITTEST + " || true"),
            step.replace(name, name + "        continue-on-error: true\n"),
        )
        for mutated in mutations:
            with self.assertRaises(AssertionError):
                assert_combined_unittest(mutated)

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
