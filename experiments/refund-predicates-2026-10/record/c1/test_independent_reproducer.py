"""Black-box tests for independent_reproducer.py.

Expectations come only from expectations.json, refusal-expectations.json and
CONTRACT.md. The reproducer is exercised through its CLI, never imported.
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

HERE = Path(__file__).resolve().parent
TOOL = HERE / "independent_reproducer.py"
SIZE_LIMIT = 1048576
NON_CLAIMS = [
    "no-live-execution",
    "no-runtime-coverage-proof",
    "no-issuer-authentication",
    "approval-digest-not-verified-against-content",
    "no-C2-or-C3",
    "no-adequacy-score",
]


def run(path, env=None):
    return subprocess.run(
        [sys.executable, "-B", str(TOOL), str(path)],
        capture_output=True, timeout=60, check=False, env=env,
    )


def load(name):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


class FrozenInputs(unittest.TestCase):
    def test_hash_index_matches_bytes(self):
        index = load("fixtures.sha256.json")
        on_disk = sorted(
            str(p.relative_to(HERE))
            for d in ("fixtures", "invalid") for p in (HERE / d).glob("*.json")
        )
        self.assertEqual(sorted(index), on_disk)
        for rel, digest in index.items():
            self.assertEqual(hashlib.sha256((HERE / rel).read_bytes()).hexdigest(), digest, rel)

    def test_every_semantic_fixture_has_one_expectation(self):
        stems = sorted(p.stem for p in (HERE / "fixtures").glob("*.json"))
        self.assertEqual(sorted(load("expectations.json")), stems)

    def test_every_refusal_file_has_one_expectation(self):
        names = sorted(p.name for p in (HERE / "invalid").glob("*.json"))
        listed = sorted(k for k in load("refusal-expectations.json") if not k.startswith("@"))
        self.assertEqual(listed, names)


class SemanticFixtures(unittest.TestCase):
    def test_each_fixture_report(self):
        for case_id, want in load("expectations.json").items():
            with self.subTest(case_id=case_id):
                path = HERE / "fixtures" / (case_id + ".json")
                proc = run(path)
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertEqual(proc.stderr, b"")
                report = json.loads(proc.stdout)
                self.assertEqual(sorted(report), sorted([
                    "schema", "case_id", "packet_sha256", "source_class", "evidence_basis",
                    "c1", "no_forbidden_dispatch", "non_claims"]))
                self.assertEqual(report["schema"], "refund.c1-report.v0")
                self.assertEqual(report["case_id"], case_id)
                self.assertEqual(report["packet_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertEqual(report["source_class"], "synthetic_fixture")
                self.assertEqual(report["evidence_basis"], "fixture_declared")
                self.assertEqual(report["non_claims"], NON_CLAIMS)
                self.assertEqual(report["c1"], {
                    "applicable": want["c1_applicable"],
                    "result": want["c1_result"],
                    "reason": want["c1_reason"]})
                self.assertEqual(report["no_forbidden_dispatch"], {
                    "result": want["no_forbidden_dispatch"],
                    "reason": want["no_forbidden_dispatch_reason"]})


class Refusals(unittest.TestCase):
    def assert_refused(self, path):
        # The interpreter itself exits 2 for a missing script; require the tool's own refusal.
        self.assertTrue(TOOL.is_file(), "reproducer missing")
        proc = run(path)
        self.assertEqual(proc.returncode, 2, proc.stdout)
        self.assertEqual(proc.stdout, b"")
        self.assertTrue(proc.stderr.startswith(b"refused: "), proc.stderr)

    def test_each_invalid_file(self):
        for name, want in load("refusal-expectations.json").items():
            if name.startswith("@"):
                continue
            with self.subTest(name=name):
                self.assertEqual(want, {"exit": 2, "stdout": ""})
                self.assert_refused(HERE / "invalid" / name)

    def padded(self, size):
        recipe = load("refusal-expectations.json")["@oversize-valid-json"]["recipe"]
        raw = (HERE / recipe["source"]).read_bytes()
        tmp = tempfile.NamedTemporaryFile(suffix=".json", delete=False)
        tmp.write(raw + b" " * (size - len(raw)))
        tmp.close()
        self.addCleanup(Path(tmp.name).unlink)
        return Path(tmp.name), recipe

    def test_oversize_valid_json_recipe(self):
        path, recipe = self.padded(load("refusal-expectations.json")["@oversize-valid-json"]["recipe"]["pad_to"])
        self.assertEqual(recipe["pad_to"], SIZE_LIMIT + 1)
        self.assertEqual(path.stat().st_size, SIZE_LIMIT + 1)
        self.assert_refused(path)

    def test_exactly_limit_is_accepted(self):
        path, _ = self.padded(SIZE_LIMIT)
        proc = run(path)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(json.loads(proc.stdout)["c1"]["result"], "ESTABLISHED")

    def test_missing_file_and_wrong_argc_refuse(self):
        self.assert_refused(HERE / "does-not-exist.json")
        proc = subprocess.run([sys.executable, "-B", str(TOOL)], capture_output=True, timeout=60, check=False)
        self.assertEqual(proc.returncode, 2)
        self.assertEqual(proc.stdout, b"")
        self.assertTrue(proc.stderr.startswith(b"refused: "), proc.stderr)


class RepairRegressions(unittest.TestCase):
    """Supplementary F1-F3 cases from the final review; not frozen pre-reader expectations.

    Packets are derived from fixtures/P1.json by editing only the dispatch side.
    Parse convention per CONTRACT-ADDENDUM: fraction/exponent tokens must be finite
    binary64; integer tokens have at most 4300 decimal digits, independent of the
    host's int/str conversion setting; protected money fields stay strict integers.
    """

    def packet(self, extra=None, amount=None):
        text = (HERE / "fixtures" / "P1.json").read_text(encoding="utf-8")
        cut = text.rindex('"payment_id": "payment-1"')
        head, tail = text[:cut], text[cut:]
        if extra is not None:
            tail = tail.replace('"payment-1"', '"payment-1",\n      "extra": ' + extra, 1)
        if amount is not None:
            at = head.rindex('"amount": 100')
            head = head[:at] + '"amount": ' + amount + head[at + len('"amount": 100'):]
        tmp = tempfile.NamedTemporaryFile(suffix=".json", delete=False)
        tmp.write((head + tail).encode("utf-8"))
        tmp.close()
        self.addCleanup(Path(tmp.name).unlink)
        return Path(tmp.name)

    def assert_refused(self, path, env=None):
        proc = run(path, env)
        self.assertEqual(proc.returncode, 2, proc.stderr)
        self.assertEqual(proc.stdout, b"")
        self.assertTrue(proc.stderr.startswith(b"refused: "), proc.stderr)
        self.assertNotIn(b"Traceback", proc.stderr)

    def assert_unknown_field(self, path, env=None):
        proc = run(path, env)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        report = json.loads(proc.stdout)
        self.assertEqual(report["c1"], {"applicable": True, "result": "NOT_ESTABLISHED",
                                        "reason": "unknown_action_fields"})
        self.assertEqual(report["packet_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())

    # F1: an escaped surrogate pair names the same key as the literal character.
    def test_f1_literal_and_escaped_astral_key_is_duplicate(self):
        self.assert_refused(self.packet(extra='{"\U0001F600": 0, "\\ud83d\\ude00": 1}'))

    def test_f1_two_escaped_spellings_are_duplicate(self):
        self.assert_refused(self.packet(extra='{"\\ud83d\\ude00": 0, "\\uD83D\\uDE00": 1}'))

    def test_f1_distinct_astral_keys_are_accepted(self):
        self.assert_unknown_field(self.packet(extra='{"\U0001F600": 0, "\\ud83d\\ude01": 1}'))

    # F2: oversized integer tokens refuse cleanly, independent of host settings.
    def test_f2_5000_digit_protected_amount_refuses(self):
        self.assert_refused(self.packet(amount="1" * 5000))

    def test_f2_5000_digit_unknown_integer_refuses(self):
        self.assert_refused(self.packet(extra="1" * 5000))

    def test_f2_4301_digit_unknown_integer_refuses(self):
        self.assert_refused(self.packet(extra="-" + "9" * 4301))

    def test_f2_4300_digit_unknown_integer_is_accepted_under_low_host_limit(self):
        env = dict(os.environ, PYTHONINTMAXSTRDIGITS="640")
        self.assert_unknown_field(self.packet(extra="-" + "9" * 4300), env)

    def test_f2_4300_digit_amount_refuses_on_range_not_parse(self):
        env = dict(os.environ, PYTHONINTMAXSTRDIGITS="640")
        proc = run(self.packet(amount="9" * 4300), env)
        self.assertEqual(proc.returncode, 2)
        self.assertIn(b"dispatch.action.amount", proc.stderr)

    # F3: fraction/exponent tokens must be finite binary64.
    def test_f3_overflowing_exponent_refuses(self):
        for token in ("1e9999", "-1e9999", "1.8e308", "1" * 400 + ".0"):
            with self.subTest(token=token[:12]):
                self.assert_refused(self.packet(extra=token))

    def test_f3_finite_fractions_are_accepted(self):
        for token in ("1.25", "1.7976931348623157e308", "1e-9999", "-0.0"):
            with self.subTest(token=token):
                self.assert_unknown_field(self.packet(extra=token))

    def test_f3_money_stays_strict_integer(self):
        for token in ("100.0", "1e2", "1.0E2"):
            with self.subTest(token=token):
                self.assert_refused(self.packet(amount=token))


class Independence(unittest.TestCase):
    def test_imports_are_stdlib_only(self):
        tree = ast.parse(TOOL.read_text(encoding="utf-8"))
        names = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names.update(a.name.split(".")[0] for a in node.names)
            elif isinstance(node, ast.ImportFrom):
                names.add((node.module or "").split(".")[0])
        self.assertTrue(names <= set(sys.stdlib_module_names), names)
        self.assertNotIn("json", names, "own decoder and encoder: no json module")


if __name__ == "__main__":
    unittest.main()
