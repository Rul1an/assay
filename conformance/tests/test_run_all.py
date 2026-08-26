#!/usr/bin/env python3
"""Behavioural tests for conformance/run_all.py. Standard library only.

    python3 conformance/tests/test_run_all.py

These exist because a runner that cannot report failure is worthless, and
because the grading distinction this runner is built on -- "checked and
disagreed" versus "could not check" -- is exactly the kind of thing that
silently inverts. Every failure path below is exercised against a fake child
process rather than asserted in prose.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import time
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import bounded_run as br  # noqa: E402
import run_all  # noqa: E402


def _fake_suite(**kw):
    s = {"id": "fake", "path": "examples/mcp-jsonrpc-id-conformance",
         "expect_status": "contradiction"}
    s.update(kw)
    return s


class _Child:
    """Stands in for _run_capped: returns a canned CompletedProcess."""

    def __init__(self, stdout="", stderr="", returncode=0, raises=None):
        self.args = (stdout, stderr, returncode, raises)

    def __call__(self, cmd, cwd, timeout):
        stdout, stderr, rc, raises = self.args
        if raises is not None:
            raise raises
        return subprocess.CompletedProcess(cmd, rc, stdout, stderr)


class StdlibClassification(unittest.TestCase):
    """The proved / false / unproved split, one test per boundary."""

    def _run(self, child):
        orig, run_all._run_capped = run_all._run_capped, child
        try:
            return run_all._stdlib_jsonrpc(_fake_suite())
        finally:
            run_all._run_capped = orig

    def test_matching_status_and_clean_exit_is_proved(self):
        g, _ = self._run(_Child(json.dumps({"status": "contradiction", "summary": {}})))
        self.assertEqual(g, run_all.PROVED)

    def test_mismatching_status_is_false_not_unproved(self):
        g, d = self._run(_Child(json.dumps({"status": "no_contradiction"})))
        self.assertEqual(g, run_all.FALSE)
        self.assertIn("no_contradiction", d)

    def test_mismatch_reported_through_a_nonzero_exit_is_still_false(self):
        # The regression this test exists for: a checker that signals a real
        # disagreement by exiting nonzero while still emitting a usable report
        # must not be filed as "could not check".
        g, _ = self._run(_Child(json.dumps({"status": "no_contradiction"}), returncode=1))
        self.assertEqual(g, run_all.FALSE)

    def test_nonzero_exit_with_no_parseable_report_is_unproved(self):
        g, d = self._run(_Child("not json", "boom", returncode=2))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("boom", d)

    def test_clean_exit_with_unparsable_report_is_unproved(self):
        g, _ = self._run(_Child("not json"))
        self.assertEqual(g, run_all.UNPROVED)

    def test_matching_status_but_nonzero_exit_is_unproved(self):
        g, _ = self._run(_Child(json.dumps({"status": "contradiction"}), returncode=3))
        self.assertEqual(g, run_all.UNPROVED)

    def test_output_over_the_cap_is_unproved_and_says_so(self):
        g, d = self._run(_Child(raises=run_all._OutputTooLarge()))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("exceeded", d)

    def test_timeout_is_unproved(self):
        g, _ = self._run(_Child(raises=subprocess.TimeoutExpired("x", 1)))
        self.assertEqual(g, run_all.UNPROVED)

    def test_incomplete_output_drain_is_unproved(self):
        g, d = self._run(_Child(raises=br._OutputDrainIncomplete("not EOF")))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("runner could not complete", d)

    def test_missing_checker_is_unproved(self):
        g, d = run_all._stdlib_jsonrpc(_fake_suite(path="does/not/exist"))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("absent", d)


class UnusableReports(unittest.TestCase):
    """A report this runner cannot compare is `could not check`, never a disagreement."""

    def _run(self, child):
        orig, run_all._run_capped = run_all._run_capped, child
        try:
            return run_all._stdlib_jsonrpc(_fake_suite())
        finally:
            run_all._run_capped = orig

    def test_a_json_array_is_unproved_and_does_not_crash(self):
        # Before the fix this raised AttributeError and took the whole run with it.
        g, d = self._run(_Child("[]"))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("not an object", d)

    def test_an_object_with_no_status_is_unproved_not_false(self):
        g, d = self._run(_Child("{}"))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("no string", d)

    def test_a_non_string_status_is_unproved_not_false(self):
        g, _ = self._run(_Child('{"status": 5}'))
        self.assertEqual(g, run_all.UNPROVED)

    def test_a_string_status_that_disagrees_is_still_false(self):
        # The guard must not swallow a genuine disagreement.
        g, _ = self._run(_Child('{"status": "no_contradiction"}'))
        self.assertEqual(g, run_all.FALSE)


class ProcessTreeContainment(unittest.TestCase):
    def test_a_descendant_holding_the_pipes_is_reaped(self):
        # A forked descendant inherits stdout and can keep writing past the ceiling
        # after the direct child is killed, so the whole group must be stopped.
        script = (
            "import subprocess, sys, time\n"
            "subprocess.Popen([sys.executable, '-c',"
            " \"import sys,time\\nfor _ in range(200):\\n sys.stdout.write('x'*4096)\\n"
            " sys.stdout.flush()\\n time.sleep(0.05)\"])\n"
            "time.sleep(0.2)\n"
        )
        with tempfile.TemporaryDirectory() as d:
            start = time.monotonic()
            p = run_all._run_capped([sys.executable, "-c", script], Path(d), timeout=20)
            elapsed = time.monotonic() - start
        # It must return promptly rather than waiting on the descendant's lifetime.
        self.assertLess(elapsed, 15, "runner did not reap the descendant")
        self.assertIsNotNone(p)


class CargoClassification(unittest.TestCase):
    def _run(self, child):
        suite = {"crate": "c", "cargo_target_flag": "--test", "cargo_target": "t"}
        orig, run_all._run_capped = run_all._run_capped, child
        try:
            return run_all._cargo(suite)
        finally:
            run_all._run_capped = orig

    def test_zero_tests_selected_is_unproved_not_proved(self):
        # cargo exits 0 when its filter matches nothing, so a green exit alone
        # would report a suite that never ran as a suite that agreed.
        g, d = self._run(_Child("running 0 tests\n", returncode=0))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("NO tests", d)

    def test_passing_tests_report_the_count(self):
        g, d = self._run(_Child("test result: ok. 7 passed; 0 failed\n"))
        self.assertEqual(g, run_all.PROVED)
        self.assertIn("7 tests", d)

    def test_assertion_failure_is_false(self):
        g, _ = self._run(_Child("test result: FAILED. 1 passed; 2 failed\n", returncode=101))
        self.assertEqual(g, run_all.FALSE)

    def test_compile_error_is_unproved_not_false(self):
        # A build that never ran is an execution condition, not a disagreement.
        g, _ = self._run(_Child("", "error[E0433]: failed to resolve", returncode=101))
        self.assertEqual(g, run_all.UNPROVED)

    def test_missing_toolchain_is_unproved(self):
        g, d = self._run(_Child(raises=FileNotFoundError()))
        self.assertEqual(g, run_all.UNPROVED)
        self.assertIn("cargo", d)


class ExitCodePrecedence(unittest.TestCase):
    """false outranks unproved, and non-run states never set an exit code."""

    def test_rank_orders_false_above_unproved(self):
        self.assertGreater(run_all.RANK[run_all.FALSE], run_all.RANK[run_all.UNPROVED])

    def test_declared_non_run_states_rank_with_proved(self):
        for state in (run_all.NEEDS_CANDIDATE, run_all.NOT_SELECTED, run_all.EXTERNAL):
            self.assertEqual(run_all.RANK[state], run_all.RANK[run_all.PROVED], state)


class OutputCap(unittest.TestCase):
    def test_a_child_that_floods_is_killed_and_raises(self):
        with tempfile.TemporaryDirectory() as d:
            with self.assertRaises(run_all._OutputTooLarge):
                run_all._run_capped(
                    [sys.executable, "-c",
                     "import sys\nwhile True: sys.stdout.write('x'*65536)"],
                    Path(d), timeout=60)

    def test_normal_output_passes_through(self):
        with tempfile.TemporaryDirectory() as d:
            p = run_all._run_capped([sys.executable, "-c", "print('hi')"], Path(d), timeout=30)
            self.assertEqual(p.returncode, 0)
            self.assertIn("hi", p.stdout)


class EndToEnd(unittest.TestCase):
    def test_json_mode_is_wellformed_and_counts_the_non_run_suites(self):
        p = subprocess.run([sys.executable, str(run_all.__file__), "--json"],
                           capture_output=True, text=True, timeout=300)
        self.assertIn(p.returncode, (0, 1, 2))
        d = json.loads(p.stdout)
        self.assertEqual(d["schema"], "assay.conformance.run_all.v1")
        self.assertEqual(len(d["suites"]), d["ran"] + d["not_run"])
        self.assertGreater(d["not_run"], 0, "non-run suites must be counted, never omitted")

    def test_json_mode_marks_partial_execution_instead_of_reporting_complete(self):
        p = subprocess.run([sys.executable, str(run_all.__file__), "--json"],
                           capture_output=True, text=True, timeout=300)
        self.assertIn(p.returncode, (0, 1, 2))
        d = json.loads(p.stdout)
        self.assertEqual(d["declared"], len(d["suites"]))
        self.assertEqual(d["executed"], d["ran"])
        self.assertLess(d["executed"], d["declared"])
        self.assertIs(d["complete"], False)

    def test_v1_corpus_is_inventoried_with_an_honest_non_run_state(self):
        p = subprocess.run([sys.executable, str(run_all.__file__), "--json"],
                           capture_output=True, text=True, timeout=300)
        d = json.loads(p.stdout)
        v1 = next(s for s in d["suites"] if s["id"] == "privileged-mcp-action-v1")
        self.assertEqual(v1["grade"], run_all.NEEDS_CANDIDATE)
        self.assertIn("candidate", v1["detail"].lower())

    def test_v1_inventory_path_and_vector_count_match_its_manifest(self):
        suite = next(s for s in run_all.SUITES if s["id"] == "privileged-mcp-action-v1")
        manifest_path = run_all.REPO / suite["path"] / "MANIFEST.json"
        self.assertTrue(manifest_path.is_file(), manifest_path)
        manifest = json.loads(manifest_path.read_text())
        self.assertEqual(suite["vectors"], sum(manifest["counts"].values()))

    def test_text_mode_always_names_the_suites_that_did_not_run(self):
        p = subprocess.run([sys.executable, str(run_all.__file__)],
                           capture_output=True, text=True, timeout=300)
        self.assertIn("NOT RUN (declared, not a pass)", p.stdout)


class CompletenessPolicy(unittest.TestCase):
    """complete is executed == declared. The executed grade is a different fact."""

    def test_zero_of_n_is_incomplete_and_has_no_executed_grade(self):
        results = [
            {"id": "a", "grade": run_all.NEEDS_CANDIDATE},
            {"id": "b", "grade": run_all.EXTERNAL},
        ]
        s = run_all.summarize(results)
        self.assertEqual(s["declared"], 2)
        self.assertEqual(s["executed"], 0)
        self.assertIs(s["complete"], False)
        self.assertIs(s["worst_executed_grade"], None)
        self.assertEqual([r["grade"] for r in s["not_run"]],
                         [run_all.NEEDS_CANDIDATE, run_all.EXTERNAL])
        self.assertEqual(s["ran"], [])
        self.assertEqual(run_all.exit_status(results, require_complete=False), 0)

    def test_one_of_n_is_incomplete_even_when_executed_grade_is_proved(self):
        results = [
            {"id": "ran", "grade": run_all.PROVED},
            {"id": "skip", "grade": run_all.NOT_SELECTED},
            {"id": "need", "grade": run_all.NEEDS_CANDIDATE},
        ]
        s = run_all.summarize(results)
        self.assertEqual(s["declared"], 3)
        self.assertEqual(s["executed"], 1)
        self.assertIs(s["complete"], False)
        self.assertEqual(s["worst_executed_grade"], run_all.PROVED)
        self.assertEqual([r["id"] for r in s["not_run"]], ["skip", "need"])
        self.assertEqual(run_all.exit_status(results, require_complete=False), 0)

    def test_require_complete_exit_uses_complete_not_the_executed_grade(self):
        # Mutation bite: deleting the completeness exit check. Uses NOT_SELECTED
        # so a "count needs_candidate as executed" mutant cannot satisfy this.
        results = [
            {"id": "ran", "grade": run_all.PROVED},
            {"id": "skip", "grade": run_all.NOT_SELECTED},
        ]
        self.assertIs(run_all.summarize(results)["complete"], False)
        self.assertEqual(run_all.summarize(results)["worst_executed_grade"], run_all.PROVED)
        self.assertEqual(run_all.exit_status(results, require_complete=False), 0)
        self.assertEqual(run_all.exit_status(results, require_complete=True), 3)
        self.assertEqual(run_all.REQUIRE_COMPLETE_EXIT, 3)

    def test_mixed_false_unproved_incomplete_exits_false_first(self):
        # FALSE + UNPROVED + a not-run suite: measured disagreement wins, not 2 or 3.
        results = [
            {"id": "bad", "grade": run_all.FALSE},
            {"id": "stuck", "grade": run_all.UNPROVED},
            {"id": "skip", "grade": run_all.NOT_SELECTED},
        ]
        self.assertIs(run_all.summarize(results)["complete"], False)
        self.assertEqual(run_all.summarize(results)["worst_executed_grade"], run_all.FALSE)
        code = run_all.exit_status(results, require_complete=True)
        self.assertEqual(code, 1)
        self.assertNotEqual(code, 2)
        self.assertNotEqual(code, 3)

    def test_require_complete_does_not_mask_incomplete_false(self):
        results = [
            {"id": "ran", "grade": run_all.FALSE},
            {"id": "skip", "grade": run_all.NOT_SELECTED},
        ]
        self.assertIs(run_all.summarize(results)["complete"], False)
        self.assertEqual(run_all.summarize(results)["worst_executed_grade"], run_all.FALSE)
        self.assertEqual(run_all.exit_status(results, require_complete=True), 1)

    def test_require_complete_does_not_mask_incomplete_unproved(self):
        results = [
            {"id": "ran", "grade": run_all.UNPROVED},
            {"id": "skip", "grade": run_all.NOT_SELECTED},
        ]
        self.assertIs(run_all.summarize(results)["complete"], False)
        self.assertEqual(run_all.summarize(results)["worst_executed_grade"], run_all.UNPROVED)
        self.assertEqual(run_all.exit_status(results, require_complete=True), 2)

    def test_n_of_n_is_complete_and_require_complete_keeps_grade_exit(self):
        proved = [
            {"id": "a", "grade": run_all.PROVED},
            {"id": "b", "grade": run_all.PROVED},
        ]
        s = run_all.summarize(proved)
        self.assertEqual(s["executed"], s["declared"])
        self.assertIs(s["complete"], True)
        self.assertEqual(s["worst_executed_grade"], run_all.PROVED)
        self.assertEqual(s["not_run"], [])
        self.assertEqual(run_all.exit_status(proved, require_complete=True), 0)

        mixed = [
            {"id": "a", "grade": run_all.PROVED},
            {"id": "b", "grade": run_all.FALSE},
        ]
        self.assertIs(run_all.summarize(mixed)["complete"], True)
        self.assertEqual(run_all.summarize(mixed)["worst_executed_grade"], run_all.FALSE)
        self.assertEqual(run_all.exit_status(mixed, require_complete=True), 1)

    def test_not_run_states_are_not_counted_as_executed(self):
        # Mutation bite: treating a not-run suite as executed makes 1/N look complete.
        results = [
            {"id": "ran", "grade": run_all.PROVED},
            {"id": "need", "grade": run_all.NEEDS_CANDIDATE},
        ]
        s = run_all.summarize(results)
        self.assertEqual(s["executed"], 1)
        self.assertEqual(s["declared"], 2)
        self.assertIs(s["complete"], False)
        self.assertEqual(s["not_run"][0]["grade"], run_all.NEEDS_CANDIDATE)


class RequireCompleteFlag(unittest.TestCase):
    def test_plain_json_mode_is_not_a_completeness_gate(self):
        p = subprocess.run([sys.executable, str(run_all.__file__), "--json"],
                           capture_output=True, text=True, timeout=300)
        d = json.loads(p.stdout)
        self.assertIs(d["complete"], False)
        self.assertLess(d["executed"], d["declared"])
        self.assertIn(d["worst_executed_grade"], (run_all.PROVED, run_all.UNPROVED, run_all.FALSE))
        self.assertIs(d["require_complete"], False)
        if d["worst_executed_grade"] == run_all.PROVED:
            self.assertEqual(p.returncode, 0)

    def test_require_complete_exits_nonzero_on_incomplete_without_hiding_results(self):
        # Mutation bite: deleting the completeness exit check.
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json", "--require-complete"],
            capture_output=True, text=True, timeout=300)
        self.assertEqual(p.returncode, 3)
        d = json.loads(p.stdout)
        self.assertIs(d["complete"], False)
        self.assertEqual(d["declared"], len(d["suites"]))
        self.assertGreater(d["not_run"], 0)
        self.assertTrue(any(
            s["grade"] in (run_all.NEEDS_CANDIDATE, run_all.NOT_SELECTED, run_all.EXTERNAL)
            for s in d["suites"]))

    def test_json_keeps_v1_worst_grade_key(self):
        p = subprocess.run([sys.executable, str(run_all.__file__), "--json"],
                           capture_output=True, text=True, timeout=300)
        d = json.loads(p.stdout)
        self.assertIn("worst_grade", d)
        self.assertIn(d["worst_grade"], (run_all.PROVED, run_all.UNPROVED, run_all.FALSE))

    def test_json_reports_whether_require_complete_was_requested(self):
        # JSON echo of the flag, not proof the exit rule ran.
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json", "--require-complete"],
            capture_output=True, text=True, timeout=300)
        d = json.loads(p.stdout)
        self.assertIs(d["require_complete"], True)
        plain = subprocess.run(
            [sys.executable, str(run_all.__file__), "--json"],
            capture_output=True, text=True, timeout=300)
        self.assertIs(json.loads(plain.stdout)["require_complete"], False)

    def test_plain_text_mode_still_prints_inventory_under_require_complete(self):
        p = subprocess.run(
            [sys.executable, str(run_all.__file__), "--require-complete"],
            capture_output=True, text=True, timeout=300)
        self.assertEqual(p.returncode, 3)
        self.assertIn("NOT RUN (declared, not a pass)", p.stdout)
        self.assertIn("complete: no", p.stdout)


class MainExitPrecedence(unittest.TestCase):
    """false > unproved > incomplete must hold through main(), not only exit_status()."""

    def _main(self, suites, argv):
        orig_suites, orig_argv = run_all.SUITES, sys.argv
        run_all.SUITES = suites
        sys.argv = argv
        try:
            return run_all.main()
        finally:
            run_all.SUITES = orig_suites
            sys.argv = orig_argv

    def _suite(self, ident, kind, runner=None):
        row = {"id": ident, "kind": kind, "vectors": 1, "maturity": "test",
               "path": "does/not/exist", "note": "fixture"}
        if runner is not None:
            row["runner"] = runner
        if kind == "cargo":
            row.update(crate="c", cargo_target_flag="--lib", cargo_target="t")
        return row

    def test_main_false_outranks_unproved_and_incomplete(self):
        suites = [
            self._suite("bad", "stdlib", lambda _s: (run_all.FALSE, "disagreed")),
            self._suite("stuck", "stdlib", lambda _s: (run_all.UNPROVED, "stuck")),
            self._suite("skip", "cargo"),
        ]
        self.assertEqual(self._main(suites, ["run_all.py", "--require-complete"]), 1)

    def test_main_unproved_outranks_incomplete(self):
        suites = [
            self._suite("stuck", "stdlib", lambda _s: (run_all.UNPROVED, "stuck")),
            self._suite("skip", "cargo"),
        ]
        self.assertEqual(self._main(suites, ["run_all.py", "--require-complete"]), 2)

    def test_main_incomplete_is_exit_three_when_nothing_worse_ran(self):
        suites = [
            self._suite("ok", "stdlib", lambda _s: (run_all.PROVED, "ok")),
            self._suite("skip", "cargo"),
        ]
        self.assertEqual(self._main(suites, ["run_all.py", "--require-complete"]), 3)


class DocumentationTruth(unittest.TestCase):
    def test_index_says_plain_mode_is_not_a_completeness_gate(self):
        index = (run_all.REPO / "conformance/INDEX.md").read_text()
        self.assertIn("not a completeness gate", index)
        self.assertIn("--require-complete", index)
        self.assertIn("--completion-scope required", index)

    def test_acm_language_is_aligned_not_awarded_and_does_not_narrow_artifacts_to_code(self):
        documents = (
            run_all.REPO / "conformance/INDEX.md",
            run_all.REPO / "conformance/privileged-mcp-action-v0/CONFORMANCE-PROTOCOL.md",
        )
        for path in documents:
            text = path.read_text()
            self.assertIn("ACM-aligned", text, path)
            self.assertIn("not ACM-awarded", text, path)
            self.assertNotIn("ACM assumes the author-supplied artifact is the author's *code*", text,
                             path)
            self.assertNotIn("badge a run earns", text, path)
        index = documents[0].read_text()
        self.assertNotIn("shipped vectors is *Functional*", index)


class PrivilegedMcpActionLanes(unittest.TestCase):
    """Four in-tree lanes are independent registry rows, not one cargo id."""

    PRODUCER = "privileged-mcp-action-producer"
    VERIFIER = "privileged-mcp-action-verifier"
    PROJECTION = "privileged-mcp-action-projection"
    E2E = "privileged-mcp-action-e2e"
    CANDIDATE = "privileged-mcp-action-v0"
    LOCAL = (PRODUCER, VERIFIER, PROJECTION, E2E)

    def test_four_lane_ids_are_registered_and_distinct_from_the_candidate(self):
        ids = [s["id"] for s in run_all.SUITES]
        for ident in self.LOCAL:
            self.assertIn(ident, ids, ident)
        self.assertEqual(len(set(self.LOCAL)), 4)
        self.assertNotIn(self.CANDIDATE, self.LOCAL)
        self.assertIn(self.CANDIDATE, ids)

    def test_candidate_lane_stays_needs_candidate(self):
        suite = next(s for s in run_all.SUITES if s["id"] == self.CANDIDATE)
        self.assertEqual(suite["kind"], run_all.NEEDS_CANDIDATE)

    def test_in_tree_verifier_is_not_named_privileged_mcp_action_v0(self):
        verifier = next(s for s in run_all.SUITES if s["id"] == self.VERIFIER)
        self.assertNotEqual(verifier["id"], self.CANDIDATE)
        self.assertNotEqual(verifier.get("cargo_target"), "privileged-mcp-action-v0")

    def test_projection_is_not_selected_with_a_reason_and_never_passed(self):
        suite = next(s for s in run_all.SUITES if s["id"] == self.PROJECTION)
        self.assertEqual(suite["kind"], run_all.NOT_SELECTED)
        self.assertTrue(suite.get("note"), "projection must print why it is not selected")
        self.assertNotEqual(suite["kind"], run_all.PROVED)

    def test_v1_descriptor_still_shares_verify_report_v0(self):
        path = run_all.REPO / "conformance/privileged-mcp-action-v1/descriptor.json"
        descriptor = json.loads(path.read_text())
        dumped = json.dumps(descriptor)
        self.assertIn("assay.privileged_mcp_action.verify.report.v0", dumped)
        self.assertNotIn("assay.privileged_mcp_action.verify.report.v1", dumped)

    def test_exit_status_stays_the_single_authority(self):
        self.assertFalse(hasattr(run_all, "OUTCOME_EXIT"))
        source = Path(run_all.__file__).read_text()
        self.assertNotIn("OUTCOME_EXIT", source)
        self.assertEqual(run_all.exit_status([{"grade": run_all.FALSE}]), 1)
        self.assertEqual(run_all.exit_status([{"grade": run_all.UNPROVED}]), 2)
        self.assertEqual(
            run_all.exit_status(
                [{"grade": run_all.PROVED}, {"grade": run_all.NOT_SELECTED}]),
            0)
        self.assertEqual(
            run_all.exit_status(
                [{"grade": run_all.PROVED}, {"grade": run_all.NOT_SELECTED}],
                require_complete=True),
            3)
        self.assertEqual(
            run_all.exit_status(
                [{"id": "r", "grade": run_all.NOT_SELECTED, "policy": "required"}],
                require_complete=True, completion_scope="required"),
            3)

    def test_local_lanes_bind_to_cargo_not_source_inspection(self):
        source = Path(run_all.__file__).read_text()
        for banned in (
            "def _fn_body", "def _local_producer", "def _local_verifier",
            "def _local_e2e", "_LOCAL_RUNNERS",
        ):
            self.assertNotIn(banned, source, banned)
        for ident in (self.PRODUCER, self.VERIFIER, self.E2E):
            suite = next(s for s in run_all.SUITES if s["id"] == ident)
            self.assertEqual(suite["kind"], "cargo", ident)
            self.assertEqual(suite["policy"], "required", ident)
            self.assertIs(suite["runner"], run_all._cargo, ident)
            self.assertIsInstance(suite.get("test_filter"), str, ident)
            self.assertTrue(suite["test_filter"], ident)
        cargo_src = Path(run_all.__file__).read_text()
        start = cargo_src.index("def _cargo")
        end = cargo_src.index("\ndef _bind_runners", start)
        self.assertIn('"--locked"', cargo_src[start:end])
        self.assertEqual(cargo_src[start:end].count("cmd = ["), 1)

    def test_producer_assertions_are_independent_of_verifier(self):
        producer = next(s for s in run_all.SUITES if s["id"] == self.PRODUCER)
        verifier = next(s for s in run_all.SUITES if s["id"] == self.VERIFIER)
        self.assertNotEqual(producer["id"], verifier["id"])
        self.assertNotIn("lane", producer)
        self.assertNotIn("lane", verifier)
        self.assertEqual(producer["test_filter"], "producer_lane_")
        self.assertNotEqual(producer["test_filter"], verifier.get("test_filter"))
        src = (
            run_all.REPO
            / "crates/assay-cli/src/cli/commands/evidence/privileged_mcp_action.rs"
        ).read_text()
        # Every producer_lane_ test must exist and none may call the verifier.
        self.assertIn("fn producer_lane_", src)
        start = 0
        found = 0
        while True:
            i = src.find("fn producer_lane_", start)
            if i < 0:
                break
            nxt = src.find("\n    fn ", i + 1)
            body = src[i:nxt if nxt > i else i + 2000]
            self.assertNotIn("verify_bundle_report", body, body[:80])
            found += 1
            start = i + 1
        self.assertGreaterEqual(found, 3, "producer_lane_ filter selected too few tests")

    def test_incomplete_projection_cannot_be_upgraded_to_confirmed(self):
        source = Path(run_all.__file__).read_text()
        self.assertIn("producer_reported", source)
        self.assertIn("incomplete", source)
        self.assertIn("cannot be upgraded to confirmed", source.lower() + source)


    def test_plain_mode_does_not_invoke_cargo(self):
        called = []
        def boom(suite):
            called.append(suite["id"])
            raise AssertionError("plain mode must not invoke cargo")
        originals = []
        for suite in run_all.SUITES:
            if suite.get("kind") == "cargo":
                originals.append((suite, suite["runner"]))
                suite["runner"] = boom
        orig_argv = sys.argv
        sys.argv = ["run_all.py", "--json"]
        try:
            code = run_all.main()
        finally:
            for suite, runner in originals:
                suite["runner"] = runner
            sys.argv = orig_argv
        self.assertTrue(originals, "expected bound cargo runners to replace")
        self.assertEqual(called, [])
        self.assertEqual(code, 0)

    def test_removing_the_cargo_skip_is_red_without_starting_cargo(self):
        import importlib.util
        source = Path(run_all.__file__).read_text()
        skip = (
            '        elif kind == "cargo" and not args.with_cargo:\n'
            '            grade, detail = NOT_SELECTED, "rerun with --with-cargo"\n'
        )
        self.assertEqual(source.count(skip), 1)
        mutated_src = source.replace(skip, "", 1)
        here = Path(run_all.__file__).resolve().parent
        path = here / "_run_all_skip_mutation.py"
        called = []

        def boom(suite):
            called.append(suite["id"])
            raise AssertionError("plain mode must not invoke cargo")

        orig_argv = sys.argv
        sys.argv = ["run_all.py", "--json"]
        try:
            path.write_text(mutated_src)
            spec = importlib.util.spec_from_file_location(
                "run_all_skip_mutation", path)
            mod = importlib.util.module_from_spec(spec)
            spec.loader.exec_module(mod)
            for suite in mod.SUITES:
                if suite.get("kind") == "cargo":
                    suite["runner"] = boom
            with self.assertRaises(AssertionError) as ctx:
                mod.main()
            self.assertIn("plain mode must not invoke cargo", str(ctx.exception))
            self.assertTrue(called)
        finally:
            sys.argv = orig_argv
            if path.exists():
                path.unlink()

    def test_required_scope_without_with_cargo_exits_3(self):
        orig_argv = sys.argv
        sys.argv = ["run_all.py", "--json", "--require-complete",
                    "--completion-scope", "required"]
        try:
            code = run_all.main()
        finally:
            sys.argv = orig_argv
        self.assertEqual(code, 3)

    def test_self_score_is_not_the_independent_candidate(self):
        local = [s for s in run_all.SUITES if s["id"] in self.LOCAL]
        self.assertFalse(any(s["kind"] == run_all.NEEDS_CANDIDATE for s in local))
        candidate = next(s for s in run_all.SUITES if s["id"] == self.CANDIDATE)
        self.assertEqual(candidate["kind"], run_all.NEEDS_CANDIDATE)
        self.assertIn("1840", candidate.get("maturity", "") + candidate.get("note", ""))


if __name__ == "__main__":
    unittest.main(verbosity=2)
