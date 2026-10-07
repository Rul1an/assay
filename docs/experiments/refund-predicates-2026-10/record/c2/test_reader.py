"""CLI tests for the C2 synthetic reader against the frozen contract and literal oracle.

Expected values come only from the frozen files (expectations.json, refusal-expectations.json,
fixtures.sha256.json) and from constants fixed in CONTRACT.md. Nothing is computed from fixtures by a
second implementation. The independent reproducer is a separate writer's code and is neither read nor imported.
"""
import ast
import hashlib
import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import ClassVar

HERE = Path(__file__).resolve().parent
TOOL = HERE / "reader.py"
LIMIT = 1048576
ROOT_KEYS = {"schema", "case_id", "packet_sha256", "source_class", "custody", "evidence_basis",
             "effect_coverage", "epoch", "closure_contradicted", "unattributed_blocking_effect_ids",
             "operations", "non_claims"}
ROW_KEYS = {"operation_id", "result", "reason", "committed_effect_ids", "late_effect_ids",
            "commit_unobserved_effect_ids", "reversed_effect_ids", "workflow_keys", "key_hint_effect_ids"}
NON_CLAIMS = ["no-live-execution", "no-provider-semantics", "no-exactly-once", "no-refund-safety",
              "no-custody-authentication", "no-runtime-coverage-proof", "no-adequacy-score"]


def load(name):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


def run(*args):
    return subprocess.run([sys.executable, "-B", str(TOOL), *map(str, args)],
                          capture_output=True, timeout=60, check=False)


class Base(unittest.TestCase):
    def temp(self, data):
        with tempfile.NamedTemporaryFile(suffix=".json", delete=False) as handle:
            handle.write(data)
        self.addCleanup(Path(handle.name).unlink)
        return Path(handle.name)

    def assert_report(self, path, case_id, coverage, expected):
        proc = run(path)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stderr, b"")
        report = json.loads(proc.stdout)
        self.assertEqual(set(report), ROOT_KEYS)
        self.assertEqual(report["schema"], "refund.c2-report.v0")
        self.assertEqual(report["case_id"], case_id)
        self.assertEqual(report["packet_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
        self.assertEqual(report["source_class"], "synthetic_fixture")
        self.assertEqual(report["custody"], "synthetic-simulator")
        self.assertEqual(report["evidence_basis"], "fixture_declared")
        self.assertEqual(report["effect_coverage"], coverage)
        self.assertEqual(report["non_claims"], NON_CLAIMS)
        for row in report["operations"]:
            self.assertEqual(set(row), ROW_KEYS)
        for key in ("epoch", "closure_contradicted", "unattributed_blocking_effect_ids", "operations"):
            self.assertEqual(report[key], expected[key], key)
        return report

    def assert_refused(self, args, category):
        self.assertTrue(TOOL.is_file(), "reader.py absent: the interpreter's own exit 2 must not pass")
        proc = run(*args)
        self.assertEqual(proc.returncode, 2, proc.stdout)
        self.assertEqual(proc.stdout, b"")
        first = proc.stderr.split(b"\n", 1)[0]
        self.assertTrue(first.startswith(b"refused: " + category.encode() + b": "), proc.stderr)
        self.assertNotIn(b"Traceback", proc.stderr)


class FrozenInputs(unittest.TestCase):
    def test_digest_manifest_and_case_sets(self):
        index = load("fixtures.sha256.json")
        on_disk = sorted(str(p.relative_to(HERE)) for d in ("fixtures", "invalid") for p in (HERE / d).glob("*.json"))
        self.assertEqual(sorted(index), on_disk)
        self.assertEqual(len(index), 49)
        for rel, digest in index.items():
            self.assertEqual(hashlib.sha256((HERE / rel).read_bytes()).hexdigest(), digest, rel)
        self.assertEqual(sorted(load("expectations.json")), sorted(p.stem for p in (HERE / "fixtures").glob("*.json")))
        self.assertEqual(sorted(load("refusal-expectations.json")), sorted(p.stem for p in (HERE / "invalid").glob("*.json")))
        self.assertEqual(len(load("expectations.json")), 20)
        self.assertEqual(sum(len(v["operations"]) for v in load("expectations.json").values()), 25)
        self.assertEqual(len(load("refusal-expectations.json")), 29)


class SemanticPackets(Base):
    COVERAGE: ClassVar[dict] = {"PARTIAL-DOUBLE": "incomplete"}  # every other frozen packet declares complete (CONTRACT fixtures)

    def test_all_20_full_reports(self):
        for case_id, expected in load("expectations.json").items():
            with self.subTest(case_id=case_id):
                self.assert_report(HERE / "fixtures" / f"{case_id}.json", case_id,
                                   self.COVERAGE.get(case_id, "complete"), expected)


class Refusals(Base):
    def test_all_29_frozen_refusals(self):
        for name, category in load("refusal-expectations.json").items():
            with self.subTest(name=name):
                self.assert_refused([HERE / "invalid" / f"{name}.json"], category)

    def test_io_refusals(self):
        self.assert_refused([], "io")
        self.assert_refused(["a.json", "b.json"], "io")
        self.assert_refused([HERE / "does-not-exist.json"], "io")


class ParseBounds(Base):
    """Contract parse bounds not represented as frozen files; built from SINGLE-COMMIT bytes."""

    def base(self):
        return (HERE / "fixtures" / "SINGLE-COMMIT.json").read_bytes()

    def test_byte_limit_exact_and_one_over(self):
        raw = self.base()
        exact = self.temp(raw + b" " * (LIMIT - len(raw)))
        self.assertEqual(exact.stat().st_size, LIMIT)
        self.assert_report(exact, "SINGLE-COMMIT", "complete", load("expectations.json")["SINGLE-COMMIT"])
        self.assert_refused([self.temp(raw + b" " * (LIMIT + 1 - len(raw)))], "parse")

    def test_far_oversize_input_refuses_as_parse(self):
        self.assert_refused([self.temp(self.base() + b" " * (8 * LIMIT))], "parse")

    def test_integer_digit_boundary_independent_of_host_int_limit(self):
        # Confirmation (not RED): 4300-digit tokens stay schema, 4301 stay parse, under a low host limit.
        env = dict(os.environ, PYTHONINTMAXSTRDIGITS="640")
        for name, category in (("integer-4300-digits-amount", b"schema"), ("integer-4301-digits-amount", b"parse")):
            with self.subTest(name=name):
                proc = subprocess.run([sys.executable, "-B", str(TOOL), str(HERE / "invalid" / f"{name}.json")],
                                      capture_output=True, timeout=60, check=False, env=env)
                self.assertEqual(proc.returncode, 2)
                self.assertTrue(proc.stderr.startswith(b"refused: " + category + b": "), proc.stderr)

    def test_bom_and_invalid_utf8(self):
        self.assert_refused([self.temp(b"\xef\xbb\xbf" + self.base())], "parse")
        self.assert_refused([self.temp(self.base().replace(b'"synthetic-simulator"', b'"synthetic-\xff"'))], "parse")


class Independence(unittest.TestCase):
    def test_reader_imports_stdlib_only_and_no_reproducer(self):
        tree = ast.parse(TOOL.read_text(encoding="utf-8"))
        names = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names.update(a.name.split(".")[0] for a in node.names)
            elif isinstance(node, ast.ImportFrom):
                names.add((node.module or "").split(".")[0])
        self.assertTrue(names <= set(sys.stdlib_module_names), names)
        self.assertNotIn("independent_reproducer", names)


if __name__ == "__main__":
    unittest.main()
