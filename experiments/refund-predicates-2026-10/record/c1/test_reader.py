"""CLI contract tests: detect lost binding, false completeness and input acceptance."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent


class ReaderContract(unittest.TestCase):
    def invoke(self, path):
        return subprocess.run([sys.executable, str(ROOT / 'reader.py'), str(path)],
                              capture_output=True, timeout=5)

    def test_literal_semantic_cases_and_original_byte_identity(self):
        expectations = json.loads((ROOT / 'expectations.json').read_bytes())
        for name, expected in expectations.items():
            with self.subTest(name=name):
                path = ROOT / 'fixtures' / (name + '.json')
                result = self.invoke(path)
                self.assertEqual(result.returncode, 0, result.stderr.decode())
                self.assertEqual(result.stderr, b'')
                report = json.loads(result.stdout)
                self.assertEqual(report['schema'], 'refund.c1-report.v0')
                self.assertEqual(report['case_id'], name)
                self.assertEqual(report['source_class'], 'synthetic_fixture')
                self.assertEqual(report['evidence_basis'], 'fixture_declared')
                self.assertEqual(report['packet_sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
                self.assertEqual(report['c1'], {
                    'applicable': expected['c1_applicable'],
                    'result': expected['c1_result'], 'reason': expected['c1_reason']})
                self.assertEqual(report['no_forbidden_dispatch'], {
                    'result': expected['no_forbidden_dispatch'],
                    'reason': expected['no_forbidden_dispatch_reason']})
                self.assertEqual(report['non_claims'], [
                    'no-live-execution', 'no-runtime-coverage-proof',
                    'no-issuer-authentication', 'approval-digest-not-verified-against-content',
                    'no-C2-or-C3', 'no-adequacy-score'])

    def test_frozen_refusals_never_emit_success_report(self):
        cases = json.loads((ROOT / 'refusal-expectations.json').read_bytes())
        with tempfile.TemporaryDirectory() as tmp:
            for name, expected in cases.items():
                with self.subTest(name=name):
                    path = ROOT / 'invalid' / name
                    if name.startswith('@'):
                        path = Path(tmp) / 'oversize.json'
                        spec = expected['recipe']
                        raw = (ROOT / spec['source']).read_bytes()
                        path.write_bytes(raw + b' ' * (spec['pad_to'] - len(raw)))
                    result = self.invoke(path)
                    self.assertEqual(result.returncode, expected['exit'])
                    self.assertEqual(result.stdout, expected['stdout'].encode())
                    self.assertTrue(result.stderr)
                    self.assertNotIn(b'Traceback', result.stderr)

    def test_whitespace_changes_digest_not_claims(self):
        source = ROOT / 'fixtures/P1.json'
        original = self.invoke(source)
        self.assertEqual(original.returncode, 0, original.stderr.decode())
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'packet.json'
            path.write_bytes(b' \n' + source.read_bytes())
            changed = self.invoke(path)
        self.assertEqual(changed.returncode, 0, changed.stderr.decode())
        a, b = json.loads(original.stdout), json.loads(changed.stdout)
        self.assertNotEqual(a.pop('packet_sha256'), b.pop('packet_sha256'))
        self.assertEqual(a, b)

    def test_integer_token_limit_is_independent_of_host_settings(self):
        packet = json.loads((ROOT / 'fixtures/P1.json').read_bytes())
        packet['dispatch']['action']['extra'] = 'TOKEN'
        template = json.dumps(packet).encode()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'number.json'
            for host_limit in ('0', '640', '4300'):
                for digits, expected_exit in ((4300, 0), (4301, 2)):
                    with self.subTest(host_limit=host_limit, digits=digits):
                        path.write_bytes(template.replace(b'"TOKEN"', b'9' * digits))
                        result = subprocess.run(
                            [sys.executable, str(ROOT / 'reader.py'), str(path)],
                            env={**os.environ, 'PYTHONINTMAXSTRDIGITS': host_limit},
                            capture_output=True, timeout=5)
                        self.assertEqual(result.returncode, expected_exit, result.stderr)
                        self.assertNotIn(b'Traceback', result.stderr)
                        if expected_exit:
                            self.assertEqual(result.stdout, b'')
                        else:
                            report = json.loads(result.stdout)
                            self.assertEqual(report['c1']['reason'], 'unknown_action_fields')
                            self.assertEqual(report['packet_sha256'],
                                             hashlib.sha256(path.read_bytes()).hexdigest())

    def test_missing_input_has_no_report(self):
        result = self.invoke(ROOT / 'nonexistent-input.json')
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, b'')
        self.assertNotIn(b'Traceback', result.stderr)


if __name__ == '__main__':
    unittest.main()
