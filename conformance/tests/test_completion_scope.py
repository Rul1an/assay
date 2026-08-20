#!/usr/bin/env python3
"""Explicit --completion-scope contract. Does not redefine global complete.

    python3 -W error::ResourceWarning conformance/tests/test_completion_scope.py

Default --require-complete still exits 3 when global complete is false.
--completion-scope required is only valid with --require-complete and exits 3
on required_complete, not by mapping 3 to 0.
"""

from __future__ import annotations

import json
import subprocess
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import registry  # noqa: E402
import run_all  # noqa: E402

WORKFLOW = Path(__file__).resolve().parents[2] / ".github/workflows/conformance-inventory.yml"

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
    def test_workflow_invokes_both_flags_and_does_not_map_3_to_0(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("conformance/run_all.py --require-complete --completion-scope required", text)
        self.assertNotIn("continue-on-error", text)
        self.assertNotIn("paths:", text)
        self.assertNotRegex(text, r"eq 3|returncode in \(0, 3\)")

    def test_deleting_require_complete_callsite_fails(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("--require-complete", text)
        self.assertNotIn("--require-complete", text.replace("--require-complete", ""))

    def test_deleting_completion_scope_required_callsite_fails(self):
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("--completion-scope required", text)
        self.assertNotIn("--completion-scope required",
                         text.replace("--completion-scope required", ""))

    def test_registry_py_does_not_neutralize_exit_3(self):
        source = Path(registry.__file__).read_text(encoding="utf-8")
        self.assertNotRegex(source, r"returncode in \(0, 3\)")
        self.assertNotIn("if completed.returncode in (0, 3)", source)


if __name__ == "__main__":
    unittest.main(verbosity=2)
