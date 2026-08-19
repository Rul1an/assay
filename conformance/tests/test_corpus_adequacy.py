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
