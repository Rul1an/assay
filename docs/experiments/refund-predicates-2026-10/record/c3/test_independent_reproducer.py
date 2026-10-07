"""Black-box tests for the C3 independent_reproducer.py.

Oracle sources: CONTRACT.md, expectations.json, refusal-expectations.json and
boundary-expectations.json only. The reproducer is exercised through its CLI;
parser-only boundary controls import its own parse() in a fresh subprocess so
the host digit limit applies. Nothing from reader.py, test_reader.py or the
CA carrier is read or imported.
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
HOST_LIMITS = ("0", "640", "4300")
NON_CLAIMS = ["no-live-execution", "no-runtime-coverage-proof", "no-provider-authentication",
              "no-effect-truth", "no-C2", "no-adequacy-score"]
ROOT_KEYS = ["schema", "case_id", "operation_id", "packet_sha256", "source_class",
             "evidence_basis", "delivery_coverage", "claims", "non_claims"]


def load(name):
    return json.loads((HERE / name).read_text(encoding="utf-8"))


def run(path, *, limit=None, args=None):
    env = dict(os.environ)
    if limit is not None:
        env["PYTHONINTMAXSTRDIGITS"] = limit
    argv = [sys.executable, "-B", str(TOOL)] + ([str(path)] if args is None else args)
    return subprocess.run(argv, capture_output=True, timeout=120, check=False, env=env)


class Base(unittest.TestCase):
    def temp(self, data):
        handle = tempfile.NamedTemporaryFile(suffix=".json", delete=False)
        handle.write(data)
        handle.close()
        self.addCleanup(Path(handle.name).unlink)
        return Path(handle.name)

    def assert_report(self, path, claims, *, case_id, coverage="complete", limit=None):
        proc = run(path, limit=limit)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        self.assertEqual(proc.stderr, b"")
        report = json.loads(proc.stdout)
        self.assertEqual(sorted(report), sorted(ROOT_KEYS))
        self.assertEqual(report["schema"], "refund.c3-report.v0")
        self.assertEqual(report["case_id"], case_id)
        self.assertEqual(report["operation_id"], "op")
        self.assertEqual(report["packet_sha256"], hashlib.sha256(path.read_bytes()).hexdigest())
        self.assertEqual(report["source_class"], "synthetic_fixture")
        self.assertEqual(report["evidence_basis"], "fixture_declared")
        self.assertEqual(report["delivery_coverage"], coverage)
        self.assertEqual(report["non_claims"], NON_CLAIMS)
        self.assertEqual(report["claims"], claims)
        return report

    def assert_refused(self, path, category, *, limit=None, args=None):
        # The interpreter exits 2 for a missing script; require the tool's own refusal.
        self.assertTrue(TOOL.is_file(), "reproducer absent")
        proc = run(path, limit=limit, args=args)
        self.assertEqual(proc.returncode, 2, proc.stdout)
        self.assertEqual(proc.stdout, b"")
        first = proc.stderr.split(b"\n", 1)[0]
        self.assertTrue(first.startswith(b"refused: " + category.encode() + b": "), proc.stderr)
        self.assertNotIn(b"Traceback", proc.stderr)


class FrozenInputs(unittest.TestCase):
    def test_inventory_matches_bytes_and_files(self):
        index = load("fixtures.sha256.json")
        on_disk = sorted(str(p.relative_to(HERE)) for d in ("fixtures", "invalid")
                         for p in (HERE / d).glob("*.json"))
        self.assertEqual(sorted(index), on_disk)
        self.assertEqual(len(index), 76)
        for rel, digest in index.items():
            self.assertEqual(hashlib.sha256((HERE / rel).read_bytes()).hexdigest(), digest, rel)

    def test_oracle_covers_every_input_once(self):
        stems = sorted(p.stem for p in (HERE / "fixtures").glob("*.json"))
        self.assertEqual(sorted(load("expectations.json")), stems)
        self.assertEqual(len(stems), 30)
        refusals = load("refusal-expectations.json")
        files = sorted(p.stem for p in (HERE / "invalid").glob("*.json"))
        self.assertEqual(sorted(k for k in refusals if not k.startswith("@")), files)
        self.assertEqual(sorted(k for k in refusals if k.startswith("@")),
                         ["@events1001", "@oversize-valid-json"])


class SemanticFixtures(Base):
    def test_all_30_full_reports(self):
        for case_id, claims in load("expectations.json").items():
            with self.subTest(case_id=case_id):
                path = HERE / "fixtures" / (case_id + ".json")
                coverage = json.loads(path.read_bytes())["delivery_coverage"]
                self.assert_report(path, claims, case_id=case_id, coverage=coverage)

    def test_p1_identical_under_host_digit_limits(self):
        want = load("expectations.json")["P1"]
        for limit in HOST_LIMITS:
            with self.subTest(limit=limit):
                self.assert_report(HERE / "fixtures" / "P1.json", want, case_id="P1", limit=limit)


class Refusals(Base):
    def test_all_46_invalid_files_with_category(self):
        for name, category in load("refusal-expectations.json").items():
            if name.startswith("@"):
                continue
            with self.subTest(name=name):
                self.assert_refused(HERE / "invalid" / (name + ".json"), category)

    def test_parse_negatives_under_host_digit_limits(self):
        refusals = load("refusal-expectations.json")
        for name in ("integer4301", "depth33", "overflow", "nan", "surrogate-duplicate"):
            for limit in HOST_LIMITS:
                with self.subTest(name=name, limit=limit):
                    self.assert_refused(HERE / "invalid" / (name + ".json"), refusals[name], limit=limit)

    def test_io_refusals(self):
        self.assert_refused(None, "io", args=[])
        self.assert_refused(None, "io", args=["a.json", "b.json"])
        self.assert_refused(HERE / "does-not-exist.json", "io")


class DynamicBounds(Base):
    def test_byte_limit_recipe(self):
        spec = load("boundary-expectations.json")["bytes"]
        raw = (HERE / spec["source"]).read_bytes()
        pad = bytes([spec["padding_byte"]])
        self.assertEqual(pad, b" ")
        healthy = self.temp(raw + pad * (spec["healthy_size"] - len(raw)))
        self.assertEqual(healthy.stat().st_size, 1048576)
        self.assert_report(healthy, load("expectations.json")["P1"], case_id="P1")
        invalid = self.temp(raw + pad * (spec["invalid_size"] - len(raw)))
        self.assertEqual(invalid.stat().st_size, 1048577)
        self.assert_refused(invalid, load("refusal-expectations.json")[spec["refusal_key"]])

    def events_packet(self, extra):
        spec = load("boundary-expectations.json")["events"]
        root = json.loads((HERE / spec["root_source"]).read_bytes())
        events = [{"id": spec["dispatch_id"], "kind": "dispatch", "operation_id": "op", "seq": 1}]
        first, last = spec["pending_seqs"]  # inclusive range, per CONTRACT p0002..p0999
        for seq in range(first, last + 1):
            events.append({"id": spec["pending_id_format"] % seq, "kind": "provider_result",
                           "operation_id": "op", "seq": seq, "status": "pending"})
        events.append({"id": spec["report_id"], "kind": "client_report", "operation_id": "op",
                       "outcome": "unknown", "seq": last + 1})
        events.extend(extra)
        root["events"] = events
        return spec, root

    def test_1000_events_healthy(self):
        spec, root = self.events_packet([])
        self.assertEqual(len(root["events"]), spec["healthy_count"])
        path = self.temp(json.dumps(root).encode())
        self.assert_report(path, spec["expected_claims"], case_id="P1")

    def test_1001_events_refused_as_schema(self):
        # CONTRACT: append provider_result id extra, seq1000, operation op, status pending.
        spec, root = self.events_packet([{"id": "extra", "kind": "provider_result",
                                          "operation_id": "op", "seq": 1000, "status": "pending"}])
        self.assertEqual(len(root["events"]), spec["invalid_count"])
        path = self.temp(json.dumps(root).encode())
        self.assert_refused(path, load("refusal-expectations.json")[spec["refusal_key"]])

    def test_id128_healthy_and_id129_schema(self):
        root = json.loads((HERE / "fixtures" / "P1.json").read_bytes())
        root["case_id"] = "x" * 128
        path = self.temp(json.dumps(root).encode())
        self.assert_report(path, load("expectations.json")["P1"], case_id="x" * 128)
        self.assertEqual(len(json.loads((HERE / "invalid" / "long-id.json").read_bytes())["case_id"]), 129)


PARSE_DRIVER = r"""
import sys
sys.path.insert(0, sys.argv[1])
import independent_reproducer as m
raw = open(sys.argv[2], 'rb').read()
try:
    value = m.parse(raw)
except m.Refused as exc:
    print('REFUSED', exc.category)
    sys.exit(0)
check = sys.argv[3]
if check == 'int4300':
    ok = value == {"x": 10 ** 4300 - 1}
elif check == 'depth32':
    node, depth = value, 1
    while isinstance(node, list):
        node, depth = node[0], depth + 1
    ok = node == 0 and depth == 33  # 32 containers wrap the scalar
elif check == 'finite':
    ok = value == {"x": 1e308}
elif check == 'quoted':
    ok = value == [[["[[["]]]
else:
    ok = False
print('ACCEPTED', ok)
"""


class ParserOnlyControls(Base):
    """Accepted parse is not schema acceptance; these isolate the parse guards."""

    def parse(self, data, check, limit):
        path = self.temp(data)
        self.assertTrue(TOOL.is_file(), "reproducer absent")
        env = dict(os.environ, PYTHONINTMAXSTRDIGITS=limit)
        proc = subprocess.run([sys.executable, "-B", "-c", PARSE_DRIVER, str(HERE), str(path), check],
                              capture_output=True, timeout=120, check=False, env=env)
        self.assertEqual(proc.returncode, 0, proc.stderr)
        return proc.stdout.decode().strip()

    def test_boundaries_under_host_limits(self):
        spec = load("boundary-expectations.json")["parser"]
        nine = lambda n: b'{"x":' + b"9" * n + b"}"  # noqa: E731
        nest = lambda n: b"[" * n + b"0" + b"]" * n  # noqa: E731
        token = lambda t: b'{"x":' + t.encode() + b"}"  # noqa: E731
        cases = [
            (nine(spec["integer_healthy_digits"]), "int4300", "ACCEPTED True"),
            (nine(spec["integer_invalid_digits"]), "int4300", "REFUSED parse"),
            (nest(spec["depth_healthy"]), "depth32", "ACCEPTED True"),
            (nest(spec["depth_invalid"]), "depth32", "REFUSED parse"),
            (token(spec["finite_token"]), "finite", "ACCEPTED True"),
            (token(spec["overflow_token"]), "finite", "REFUSED parse"),
            (b"[[[" + spec["quoted_brackets"].encode() + b"]]]", "quoted", "ACCEPTED True"),
        ]
        for limit in HOST_LIMITS:
            for data, check, want in cases:
                with self.subTest(limit=limit, check=check, size=len(data)):
                    self.assertEqual(self.parse(data, check, limit), want)

    def test_quoted_brackets_do_not_count_toward_depth(self):
        # 32 real arrays (the limit) around a string full of brackets is still accepted.
        data = b"[" * 32 + b'"' + b"[" * 64 + b'"' + b"]" * 32
        self.assertTrue(self.parse(data, "none", "4300").startswith("ACCEPTED"))


class Independence(unittest.TestCase):
    def test_imports_and_no_stdlib_json_decoding(self):
        source = TOOL.read_text(encoding="utf-8")
        tree = ast.parse(source)
        names = set()
        for node in ast.walk(tree):
            if isinstance(node, ast.Import):
                names.update(a.name.split(".")[0] for a in node.names)
            elif isinstance(node, ast.ImportFrom):
                names.add((node.module or "").split(".")[0])
        self.assertTrue(names <= set(sys.stdlib_module_names), names)
        for banned in ("reader", "ca_baseline", "observation_session", "corpus_adequacy"):
            self.assertNotIn(banned, names)
        attrs = {n.attr for n in ast.walk(tree) if isinstance(n, ast.Attribute)}
        self.assertFalse(attrs & {"loads", "load", "JSONDecoder", "raw_decode"}, "own decoder only")


if __name__ == "__main__":
    unittest.main()
