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
        oos = dict(SURVIVOR, scope="out_of_scope")
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

    def test_a_mutant_that_crashes_the_module_counts_as_killed(self):
        broken = {"label": "syntax", "anchor": 'return "ok"', "replacement": "return ??"}
        with tempfile.TemporaryDirectory() as d:
            rep = ca.run(_manifest(Path(d), {"a": [broken]}))
        self.assertEqual(rep["killed"], 1)


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
