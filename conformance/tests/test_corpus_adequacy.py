#!/usr/bin/env python3
"""Behavioural tests for conformance/corpus_adequacy.py. Standard library only.

    python3 conformance/tests/test_corpus_adequacy.py

Built against a synthetic two-rule corpus rather than a real one, so every
verdict boundary is reachable on purpose: a rule some vector discriminates, a
rule none does, a rule declared out of scope, and a rule declared equivalent.
"""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))
import corpus_adequacy as ca  # noqa: E402

IMPL = '''
def evaluate(group, inputs):
    if inputs.get("bad"):
        return "rejected"
    if inputs.get("n", 0) > 10:
        return "big"
    return "ok"
'''

VECTORS = {"vectors": [
    {"vector_id": "v1", "axis": "a", "inputs": {"bad": True}},
    {"vector_id": "v2", "axis": "a", "inputs": {"n": 1}},
]}

KILLABLE = {"label": "rejects bad input",
            "anchor": 'if inputs.get("bad"):\n        return "rejected"',
            "replacement": 'if False:\n        return "rejected"'}
# No vector carries n > 10, so nothing can distinguish this rule.
SURVIVOR = {"label": "big branch",
            "anchor": 'if inputs.get("n", 0) > 10:',
            "replacement": 'if inputs.get("n", 0) > 999999:'}


def _manifest(tmp: Path, mutants, equivalent=None, vectors=None, raw=None) -> Path:
    (tmp / "impl.py").write_text(IMPL)
    (tmp / "vectors.json").write_text(json.dumps(vectors or VECTORS))
    m = {"schema": ca.SCHEMA, "implementation": "impl.py", "entrypoint": "evaluate",
         "vectors": "vectors.json", "group_key": "axis", "id_key": "vector_id",
         "inputs_key": "inputs", "mutants": mutants, "equivalent": equivalent or {}}
    if raw:
        m.update(raw)
    p = tmp / "m.json"
    p.write_text(json.dumps(m))
    return p


class Scoring(unittest.TestCase):
    def test_a_discriminated_rule_is_killed_and_scores_100(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE]}))
        self.assertEqual((rep["killed"], rep["survived"]), (1, 0))
        self.assertEqual(rep["score_percent"], 100.0)
        self.assertTrue(rep["adequate"])

    def test_an_undistinguished_rule_survives_and_fails_the_run(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, SURVIVOR]}))
        self.assertEqual((rep["killed"], rep["survived"]), (1, 1))
        self.assertEqual(rep["score_percent"], 50.0)
        self.assertFalse(rep["adequate"])

    def test_out_of_scope_is_reported_but_never_scored(self):
        # The distinction the tool exists to keep: a rule nobody claimed is a
        # scope statement, not a hole, and must not manufacture a failure.
        oos = dict(SURVIVOR, scope="out_of_scope", reason="the corpus does not claim this rule")
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, oos]}))
        self.assertEqual(rep["survived"], 0)
        self.assertEqual(rep["unexercised_out_of_scope"], 1)
        self.assertEqual(rep["score_percent"], 100.0)
        self.assertTrue(rep["adequate"])
        self.assertIn("unexercised", [r["verdict"] for r in rep["mutants"]])

    def test_declared_equivalents_are_excluded_from_the_denominator(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE]},
                                   {"a": [{"label": "eq", "reason": "both branches return ok"}]}))
        self.assertEqual(rep["equivalent"], 1)
        self.assertEqual(rep["killed"] + rep["survived"], 1)

    def test_a_mutant_that_never_loads_is_unproved_not_killed(self):
        # Reversed deliberately on the Rust-adapter review: a mutant that never
        # loaded was never shown to the corpus, so the corpus said nothing about
        # that rule. Counting it killed lets a typo in the substitution print as
        # "rule covered". Measure a load-bearing rule with a variant that RUNS.
        broken = {"label": "syntax", "anchor": 'return "ok"', "replacement": "return ??"}
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [broken]}))
        self.assertEqual(rep["killed"], 0)
        self.assertEqual(rep["unproved"], 1)
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("never ran" in f for f in rep["failures"]), rep["failures"])


class ControlMutants(unittest.TestCase):
    """A control proves the harness detects anything. It is never scored."""

    def test_a_killed_control_does_not_inflate_the_score(self):
        ctrl = dict(KILLABLE, label="CONTROL reachability", control=True)
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [ctrl]}))
        self.assertEqual(rep["killed"], 0, "a control must not count as a kill")
        self.assertIn("control-killed", [r["verdict"] for r in rep["mutants"]])

    def test_a_surviving_control_invalidates_the_whole_run(self):
        # The distinction the control exists for: all-survivors because the corpus is
        # weak, versus all-survivors because nothing was ever measured.
        ctrl = dict(SURVIVOR, label="CONTROL reachability", control=True)
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, ctrl]}))
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("harness cannot detect" in f for f in rep["failures"]),
                        rep["failures"])

    def test_a_control_may_not_be_declared_out_of_scope(self):
        ctrl = dict(KILLABLE, label="c", control=True, scope="out_of_scope", reason="x")
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [ctrl]})
            with self.assertRaises(ca.ManifestError) as cm:
                ca.load_manifest(p)
        self.assertIn("control cannot be out_of_scope", str(cm.exception))


class KnownHoles(unittest.TestCase):
    """An acknowledged hole is pinned to one digest and expires with it."""

    def _mf(self, tmp: Path, digest_in_file, holes_for, extra_mutants=None):
        (tmp / "digest.json").write_text(json.dumps({"corpus_digest": digest_in_file}))
        muts = [dict(SURVIVOR, label="unexercised rule")] + (extra_mutants or [])
        p = _manifest(tmp, {"a": muts}, raw={
            "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
            "known_holes": {holes_for: [{"label": "unexercised rule",
                                         "reason": "no vector reaches it",
                                         "recorded": "2026-08-19"}]}})
        return p

    def test_a_hole_acknowledged_for_the_present_digest_is_not_a_survivor(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(self._mf(Path(d), "sha256:aaa", "sha256:aaa", [KILLABLE]))
        self.assertEqual(rep["survived"], 0)
        self.assertEqual(rep["known_holes"], 1)
        self.assertIn("known-hole", [r["verdict"] for r in rep["mutants"]])

    def test_the_acknowledgement_expires_when_the_corpus_moves(self):
        # The rule that stops this being an escape hatch: an acknowledgement is a
        # statement about ONE corpus, so a corpus that changes loses it.
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(self._mf(Path(d), "sha256:NEW", "sha256:OLD", [KILLABLE]))
        self.assertEqual(rep["known_holes"], 0)
        self.assertEqual(rep["survived"], 1, "the hole must reappear as a survivor")
        self.assertFalse(rep["adequate"])

    def test_an_acknowledgement_for_a_rule_now_exercised_is_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "digest.json").write_text(json.dumps({"corpus_digest": "sha256:aaa"}))
            p = _manifest(tmp, {"a": [dict(KILLABLE, label="now exercised")]}, raw={
                "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
                "known_holes": {"sha256:aaa": [{"label": "now exercised", "reason": "x",
                                                "recorded": "2026-08-19"}]}})
            rep = ca.run(p)
        self.assertFalse(rep["adequate"])
        # The message widened from "now exercises" to cover every transition away
        # from known-hole, not only becoming killed.
        self.assertTrue(any("no longer holes" in f and "now killed" in f
                            for f in rep["failures"]), rep["failures"])

    def test_the_report_does_not_claim_the_pin_is_to_the_corpus(self):
        # The wording was false and the tool printed it: with a corpus that had moved
        # it said the acknowledgement "expires the moment the corpus changes" while
        # exiting 0 at 100%. The digest is a value read from a file the manifest
        # names, never recomputed from the vectors, and the report must say so.
        with tempfile.TemporaryDirectory() as d:
            p = self._mf(Path(d), "sha256:aaa", "sha256:aaa", [KILLABLE])
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertNotIn("expires the moment the corpus changes", r.stdout)
        self.assertIn("not recomputed from the vectors", r.stdout)

    def test_pre_declared_future_digests_are_surfaced(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "digest.json").write_text(json.dumps({"corpus_digest": "sha256:aaa"}))
            p = _manifest(tmp, {"a": [KILLABLE, dict(SURVIVOR, label="hole")]}, raw={
                "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
                "known_holes": {"sha256:aaa": [{"label": "hole", "reason": "x",
                                                "recorded": "2026-08-19"}],
                                "sha256:future1": [], "sha256:future2": []}})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertIn("digests carry acknowledgements", r.stdout)

    def test_holes_outnumbering_measurements_is_stated(self):
        holes = [dict(SURVIVOR, label=f"h{i}") for i in range(4)]
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "digest.json").write_text(json.dumps({"corpus_digest": "sha256:aaa"}))
            p = _manifest(tmp, {"a": [KILLABLE] + holes}, raw={
                "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
                "known_holes": {"sha256:aaa": [{"label": f"h{i}", "reason": "x",
                                                "recorded": "2026-08-19"} for i in range(4)]}})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertIn("acknowledged as holes than are measured", r.stdout)
        self.assertIn("acknowledged holes", r.stdout.strip().splitlines()[-1])

    def test_an_acknowledgement_lingers_when_its_rule_becomes_out_of_scope(self):
        # Only one of four transitions was covered: killed. A rule that becomes
        # out_of_scope left the acknowledgement pointing at nothing, silently.
        oos = dict(SURVIVOR, label="hole", scope="out_of_scope", reason="marked oos later")
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "digest.json").write_text(json.dumps({"corpus_digest": "sha256:aaa"}))
            p = _manifest(tmp, {"a": [KILLABLE, oos]}, raw={
                "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
                "known_holes": {"sha256:aaa": [{"label": "hole", "reason": "x",
                                                "recorded": "2026-08-19"}]}})
            rep = ca.run(p)
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("no longer holes" in f for f in rep["failures"]), rep["failures"])

    def test_a_hole_without_a_reason_is_refused(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            (tmp / "digest.json").write_text(json.dumps({"corpus_digest": "sha256:aaa"}))
            p = _manifest(tmp, {"a": [KILLABLE]}, raw={
                "corpus_digest_file": "digest.json", "corpus_digest_key": "corpus_digest",
                "known_holes": {"sha256:aaa": [{"label": "x", "reason": " ",
                                                "recorded": "2026-08-19"}]}})
            with self.assertRaises(ca.ManifestError) as cm:
                ca.load_manifest(p)
        self.assertIn("stated reason", str(cm.exception))

    def test_an_all_holes_manifest_reports_no_result_rather_than_100_percent(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(self._mf(Path(d), "sha256:aaa", "sha256:aaa"))
        self.assertIsNone(rep["score_percent"], "an empty denominator is not 100%")
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("nothing was measured" in f for f in rep["failures"]))


class BatchRunner(unittest.TestCase):
    """A corpus consumed as a unit: one invocation, the summary is the outcome."""

    def _corpus(self, tmp: Path):
        (tmp / "check.py").write_text(
            "import json, sys\n"
            "doc = json.load(open(sys.argv[1]))\n"
            "fails = [c['id'] for c in doc['cases'] if c['n'] > 10]\n"
            "print(json.dumps({'ok': not fails, 'failures': fails}))\n")
        (tmp / "vectors.json").write_text(json.dumps({"cases": [
            {"id": "c1", "n": 1}, {"id": "c2", "n": 2}]}))
        m = {"schema": ca.SCHEMA, "runner": "batch", "repo_root": ".",
             "implementation_sources": ["check.py"],
             "entrypoint_command": ["python3", "check.py", "vectors.json"],
             "outcome_from": ["ok", "failures"], "vectors": "vectors.json",
             "id_key": "vector_id", "default_group": "g",
             "mutants": {"g": [
                 {"label": "threshold", "anchor": "c['n'] > 10", "replacement": "c['n'] > 1"},
                 # must actually move the summary: emptying the case list leaves
                 # `failures` empty exactly as the baseline does, and the control
                 # guard correctly refused that when it was tried.
                 {"label": "CONTROL", "control": True,
                  "anchor": "'ok': not fails", "replacement": "'ok': 'MOVED'"}]}}
        p = tmp / "m.json"
        p.write_text(json.dumps(m))
        return p

    def test_one_invocation_still_discriminates_via_the_summary(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(self._corpus(Path(d)))
        self.assertEqual(rep["runner"], "batch")
        self.assertEqual(rep["killed"], 1)
        self.assertTrue(rep["adequate"], rep["failures"])

    def test_the_source_is_restored_after_a_batch_run(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            p = self._corpus(tmp)
            before = (tmp / "check.py").read_bytes()
            ca.run(p)
            self.assertEqual((tmp / "check.py").read_bytes(), before)

    def test_a_batch_manifest_needs_no_build(self):
        # An interpreted corpus has no build step; requiring one would exclude it.
        with tempfile.TemporaryDirectory() as d:
            m = ca.load_manifest(self._corpus(Path(d)))
        self.assertEqual(m["build"], [])

    def test_an_unreadable_summary_is_a_raise_not_a_silent_pass(self):
        with tempfile.TemporaryDirectory() as d:
            tmp = Path(d)
            p = self._corpus(tmp)
            (tmp / "check.py").write_text("print('not json')\n")
            rep = ca.run(p)
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("UNMUTATED" in f for f in rep["failures"]), rep["failures"])


class Guards(unittest.TestCase):
    def test_a_group_in_the_corpus_with_no_mutants_is_a_hard_failure(self):
        v = {"vectors": VECTORS["vectors"] + [
            {"vector_id": "v3", "axis": "b", "inputs": {"n": 1}}]}
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE]}, vectors=v))
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("no declared mutants" in f for f in rep["failures"]))

    def test_a_stale_anchor_fails_rather_than_scoring_nothing(self):
        stale = {"label": "gone", "anchor": "this text is not in the impl",
                 "replacement": "nor is this"}
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, stale]}))
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("anchor not found" in f for f in rep["failures"]))

    def test_mutants_declared_for_absent_groups_fail(self):
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE], "zz": [KILLABLE]}))
        self.assertTrue(any("not in the corpus" in f for f in rep["failures"]))


class ManifestValidation(unittest.TestCase):
    def _err(self, raw):
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE]}, raw=raw)
            with self.assertRaises(ca.ManifestError) as cm:
                ca.load_manifest(p)
            return str(cm.exception)

    def test_wrong_schema_is_refused(self):
        self.assertIn("schema", self._err({"schema": "something.else"}))

    def test_no_mutants_is_refused_rather_than_scored_as_perfect(self):
        self.assertIn("no mutants", self._err({"mutants": {}}))

    def test_an_equivalence_without_a_reason_is_refused(self):
        self.assertIn("stated reason",
                      self._err({"equivalent": {"a": [{"label": "x", "reason": "  "}]}}))

    def test_a_mutant_that_changes_nothing_is_refused(self):
        self.assertIn("mutates nothing",
                      self._err({"mutants": {"a": [{"label": "noop", "anchor": "x",
                                                    "replacement": "x"}]}}))


class RuleyFindings(unittest.TestCase):
    """Regressions for the blocking review on #2538. Each one scored 100% before."""

    def test_out_of_scope_without_a_reason_is_refused(self):
        # Finding 1: 1 killable + 5 unreasoned out_of_scope printed 100% and exited 0.
        # An out_of_scope mutant leaves the denominator exactly as an equivalent one
        # does, so it carries the same obligation.
        oos = dict(SURVIVOR, scope="out_of_scope")
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE, oos]})
            with self.assertRaises(ca.ManifestError) as cm:
                ca.load_manifest(p)
        self.assertIn("stated reason", str(cm.exception))

    def test_an_empty_anchor_is_refused(self):
        # Finding 2a: "" matches everywhere, corrupts the source, and the resulting
        # import failure was then counted as a kill.
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [{"label": "empty", "anchor": "",
                                           "replacement": "# x"}]})
            with self.assertRaises(ca.ManifestError) as cm:
                ca.load_manifest(p)
        self.assertIn("anchor is empty", str(cm.exception))

    def test_an_anchor_occurring_more_than_once_fails_the_run(self):
        # Finding 2b: a substring anchor mangled the source; the breakage scored as a kill.
        dup = {"label": "substring", "anchor": "inputs", "replacement": "broken"}
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, dup]}))
        self.assertFalse(rep["adequate"])
        self.assertTrue(any("occurs" in f and "unique" in f for f in rep["failures"]),
                        rep["failures"])

    def test_the_report_states_what_the_percentage_is_a_percentage_of(self):
        # Finding 3: the fix is the published sentence, not code. 100% is 100% of what
        # the author declared, never of the rules the implementation has.
        oos = dict(SURVIVOR, scope="out_of_scope", reason="not claimed")
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [KILLABLE, oos]}))
        self.assertEqual(rep["declared_total"], 2)
        self.assertEqual(rep["out_of_scope_ratio"], 1.0)
        self.assertIn("author-declared", rep["score_means"])

    def test_the_out_of_scope_reason_is_printed_on_its_own_line(self):
        # Follow-up on the #2538 review: "each with a stated reason" without showing
        # one is an assertion. A declared equivalent already prints its reason.
        oos = dict(SURVIVOR, scope="out_of_scope", reason="UNIQUEMARKER not claimed here")
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE, oos]})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertIn("UNIQUEMARKER", r.stdout)

    def test_the_closing_line_is_qualified_when_most_rules_were_excluded(self):
        # Follow-up: the last line is what gets quoted, so at ratio > 1 it may not
        # read as unqualified success.
        oos = [dict(SURVIVOR, label=f"o{i}", anchor='return "ok"',
                    replacement=f'return "ok"  # {i}', scope="out_of_scope",
                    reason="not claimed") for i in range(3)]
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE] + oos})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertEqual(r.returncode, 0)
        last = [l for l in r.stdout.strip().splitlines() if l.strip()][-1]
        self.assertIn("DECLARED IN-SCOPE rules only", last)
        self.assertNotEqual(
            last.strip(), "mutation-adequacy check passed: every non-equivalent mutant is killed")

    def test_the_closing_line_is_unqualified_when_nothing_was_excluded(self):
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE]})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertIn("every non-equivalent mutant is killed", r.stdout)

    def test_a_majority_excluded_corpus_says_so(self):
        oos = [dict(SURVIVOR, label=f"o{i}", anchor=f'return "ok"',
                    replacement=f'return "ok"  # {i}', scope="out_of_scope",
                    reason="not claimed") for i in range(3)]
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), {"a": [KILLABLE] + oos})
            r = subprocess.run([sys.executable, str(ca.__file__), str(p)],
                               capture_output=True, text=True, timeout=120)
        self.assertIn("more rules are excluded than measured", r.stdout)


class Portability(unittest.TestCase):
    def test_a_single_argument_entrypoint_is_supported(self):
        # Found by running the tool on a second corpus: signatures differ, and a
        # fixed arity would exclude every corpus that guessed differently.
        impl = 'def check(msg):\n    return "ok" if msg.get("k") else "no"\n'
        with tempfile.TemporaryDirectory() as raw:
            d = Path(raw)
            (d / "impl.py").write_text(impl)
            (d / "vectors.json").write_text(json.dumps({"vectors": [
                {"vector_id": "v1", "msg": {"k": 1}}, {"vector_id": "v2", "msg": {}}]}))
            m = {"schema": ca.SCHEMA, "implementation": "impl.py", "entrypoint": "check",
                 "entrypoint_args": ["msg"], "vectors": "vectors.json", "id_key": "vector_id",
                 "default_group": "only",
                 "mutants": {"only": [{"label": "truthy branch",
                                       "anchor": 'if msg.get("k")', "replacement": "if False"}]}}
            p = d / "m.json"
            p.write_text(json.dumps(m))
            rep = ca.run(p)
        self.assertEqual(rep["killed"], 1)
        self.assertTrue(rep["adequate"])


class Cli(unittest.TestCase):
    def _cli(self, mutants, *args):
        with tempfile.TemporaryDirectory() as d:
            p = _manifest(Path(d), mutants)
            return subprocess.run([sys.executable, str(ca.__file__), str(p), *args],
                                  capture_output=True, text=True, timeout=120)

    def test_exit_0_when_adequate(self):
        self.assertEqual(self._cli({"a": [KILLABLE]}).returncode, 0)

    def test_exit_1_when_a_mutant_survives(self):
        self.assertEqual(self._cli({"a": [KILLABLE, SURVIVOR]}).returncode, 1)

    def test_exit_2_when_the_manifest_cannot_be_read(self):
        r = subprocess.run([sys.executable, str(ca.__file__), "/nope/missing.json"],
                           capture_output=True, text=True, timeout=60)
        self.assertEqual(r.returncode, 2)

    def test_json_mode_is_wellformed(self):
        d = json.loads(self._cli({"a": [KILLABLE]}, "--json").stdout)
        self.assertEqual(d["schema"], "assay.corpus_adequacy.report.v0")


if __name__ == "__main__":
    unittest.main(verbosity=1)
