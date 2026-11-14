"""Literal CLI oracles catch causal-prefix, coverage and parser guard removal."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

ROOT = Path(__file__).resolve().parent
NON_CLAIMS = ['no-live-execution', 'no-runtime-coverage-proof',
              'no-provider-authentication', 'no-effect-truth', 'no-C2', 'no-adequacy-score']


class ReaderContract(unittest.TestCase):
    def invoke(self, path, limit='4300'):
        return subprocess.run([sys.executable, str(ROOT/'reader.py'), str(path)],
                              env={**os.environ, 'PYTHONINTMAXSTRDIGITS': limit},
                              capture_output=True, timeout=10)

    def refuse(self, run, category):
        self.assertEqual(run.returncode, 2)
        self.assertEqual(run.stdout, b'')
        self.assertTrue(run.stderr.startswith(f'refused: {category}:'.encode()), run.stderr)
        self.assertNotIn(b'Traceback', run.stderr)

    def test_literal_full_reports(self):
        for name, claims in json.loads((ROOT/'expectations.json').read_bytes()).items():
            with self.subTest(case=name):
                path = ROOT/'fixtures'/f'{name}.json'
                run = self.invoke(path)
                self.assertEqual(run.returncode, 0, run.stderr)
                self.assertEqual(run.stderr, b'')
                packet = json.loads(path.read_bytes())
                self.assertEqual(json.loads(run.stdout), {
                    'schema': 'refund.c3-report.v0', 'case_id': name, 'operation_id': 'op',
                    'source_class': 'synthetic_fixture', 'evidence_basis': 'fixture_declared',
                    'packet_sha256': hashlib.sha256(path.read_bytes()).hexdigest(),
                    'delivery_coverage': packet['delivery_coverage'],
                    'claims': claims, 'non_claims': NON_CLAIMS})

    def test_refusal_categories(self):
        for name, category in json.loads((ROOT/'refusal-expectations.json').read_bytes()).items():
            if name.startswith('@'):
                continue
            for limit in ('0', '640', '4300'):
                with self.subTest(case=name, limit=limit):
                    self.refuse(self.invoke(ROOT/'invalid'/f'{name}.json', limit), category)

    def test_input_inventory(self):
        for name, digest in json.loads((ROOT/'fixtures.sha256.json').read_bytes()).items():
            self.assertEqual(hashlib.sha256((ROOT/name).read_bytes()).hexdigest(), digest)

    def test_io_refusal(self):
        self.refuse(self.invoke(ROOT/'does-not-exist'), 'io')

    def test_byte_limit_healthy_and_over(self):
        raw = (ROOT/'fixtures/P1.json').read_bytes()
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'input'
            path.write_bytes(raw + b' '*(1048576-len(raw)))
            run = self.invoke(path)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(json.loads(run.stdout)['packet_sha256'], hashlib.sha256(path.read_bytes()).hexdigest())
            path.write_bytes(path.read_bytes()+b' ')
            self.refuse(self.invoke(path), json.loads((ROOT/'refusal-expectations.json').read_bytes())['@oversize-valid-json'])

    def test_event_limit_healthy_and_over(self):
        p = json.loads((ROOT/'fixtures/P1.json').read_bytes())
        p['events'] = [p['events'][0]] + [
            {'id': f'p{i:04d}', 'seq': i, 'operation_id': 'op', 'kind': 'provider_result', 'status': 'pending'}
            for i in range(2, 1000)] + [
            {'id': 'r', 'seq': 1000, 'operation_id': 'op', 'kind': 'client_report', 'outcome': 'unknown'}]
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'input'
            path.write_text(json.dumps(p))
            run = self.invoke(path)
            self.assertEqual(run.returncode, 0, run.stderr)
            claim = json.loads(run.stdout)['claims'][0]
            self.assertEqual(claim['uncertainty_calibration'], {'applicable': True, 'result': 'ESTABLISHED', 'reason': 'unresolved_support'})
            self.assertEqual([claim], json.loads((ROOT/'boundary-expectations.json').read_bytes())['events']['expected_claims'])
            p['events'].append({'id': 'extra', 'seq': 1000, 'operation_id': 'op', 'kind': 'provider_result', 'status': 'pending'})
            path.write_text(json.dumps(p))
            self.refuse(self.invoke(path), json.loads((ROOT/'refusal-expectations.json').read_bytes())['@events1001'])

    def test_identifier_exact_bound(self):
        packet = json.loads((ROOT/'fixtures/P1.json').read_bytes())
        packet['case_id'] = 'x'*128
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp)/'input'
            path.write_text(json.dumps(packet))
            run = self.invoke(path)
            self.assertEqual(run.returncode, 0, run.stderr)
            self.assertEqual(json.loads(run.stdout)['case_id'], 'x'*128)

    def test_own_parser_boundaries_under_host_limits(self):
        # Own-parser controls are separate from schema acceptance; no cross-reader import.
        program = '''import importlib.util,sys
s=importlib.util.spec_from_file_location("reader_under_test",sys.argv[1]);m=importlib.util.module_from_spec(s);s.loader.exec_module(m)
assert isinstance(m.parse(b"9"*4300),int)
assert m.parse(b"1e308") == 1e308
assert m.parse(b'"[[["') == "[[["
m.parse(b"["*32+b"0"+b"]"*32)
for raw in (b"9"*4301,b"1e999",b"["*33+b"0"+b"]"*33):
 try: m.parse(raw)
 except ValueError: pass
 else: raise AssertionError("parser bound not enforced")
'''
        for limit in ('0', '640', '4300'):
            run = subprocess.run([sys.executable, '-c', program, str(ROOT/'reader.py')],
                                 env={**os.environ, 'PYTHONINTMAXSTRDIGITS': limit}, capture_output=True)
            self.assertEqual(run.returncode, 0, run.stderr)


if __name__ == '__main__':
    unittest.main()
