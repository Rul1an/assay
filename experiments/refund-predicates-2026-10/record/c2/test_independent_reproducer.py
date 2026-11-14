"""CLI contract tests: expectations come from frozen literal oracle or hand derivation.

Breaks caught: missing CLI; malformed input escaping its refusal stage; report
identity/shape drift; double counting; key attribution; precedence/closure errors.
No import of the subject or computation of expectations by subject helpers.
"""
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
SUBJECT = HERE / 'independent_reproducer.py'
NON_CLAIMS = ['no-live-execution', 'no-provider-semantics', 'no-exactly-once',
              'no-refund-safety', 'no-custody-authentication',
              'no-runtime-coverage-proof', 'no-adequacy-score']


class ReproducerCLI(unittest.TestCase):
    def invoke(self, *args):
        return subprocess.run([sys.executable, str(SUBJECT), *map(str, args)],
                              capture_output=True, timeout=10, check=False)

    def raw(self, data):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'packet.json'
            path.write_bytes(data)
            return self.invoke(path)

    def refusal(self, completed, category):
        self.assertEqual(completed.returncode, 2, completed.stderr)
        self.assertEqual(completed.stdout, b'')
        self.assertTrue(completed.stderr.splitlines()[0].startswith(
            f'refused: {category}:'.encode()), completed.stderr)

    def success(self, completed):
        self.assertEqual(completed.returncode, 0, completed.stderr)
        self.assertEqual(completed.stderr, b'')
        return json.loads(completed.stdout)

    def base(self):
        return json.loads((HERE / 'fixtures/SINGLE-COMMIT.json').read_bytes())

    def test_all_semantic_packets_full_closed_report(self):
        oracle = json.loads((HERE / 'expectations.json').read_bytes())
        self.assertEqual(len(oracle), 20)
        self.assertEqual(sum(len(x['operations']) for x in oracle.values()), 25)
        for case, expected in oracle.items():
            with self.subTest(case=case):
                path = HERE / 'fixtures' / (case + '.json')
                data = path.read_bytes()
                packet = json.loads(data)
                full = dict(expected, schema='refund.c2-report.v0', case_id=case,
                            packet_sha256=hashlib.sha256(data).hexdigest(),
                            source_class='synthetic_fixture', custody=packet['custody'],
                            evidence_basis='fixture_declared',
                            effect_coverage=packet['effect_coverage'], non_claims=NON_CLAIMS)
                self.assertEqual(self.success(self.invoke(path)), full)

    def test_all_frozen_refusals(self):
        oracle = json.loads((HERE / 'refusal-expectations.json').read_bytes())
        self.assertEqual(len(oracle), 29)
        for case, category in oracle.items():
            with self.subTest(case=case):
                self.refusal(self.invoke(HERE / 'invalid' / (case + '.json')), category)

    def test_frozen_byte_manifest(self):
        manifest = json.loads((HERE / 'fixtures.sha256.json').read_bytes())
        self.assertEqual(len(manifest), 49)
        for name, digest in manifest.items():
            with self.subTest(name=name):
                self.assertEqual(hashlib.sha256((HERE / name).read_bytes()).hexdigest(), digest)

    def test_io_arity_missing_file_directory(self):
        for args in [(), ('absent-c2-packet',), (HERE,), ('a', 'b')]:
            with self.subTest(args=args):
                self.refusal(self.invoke(*args), 'io')

    def test_parse_edges_and_stage_precedence(self):
        # Even a schema-invalid root must finish parsing before schema runs.
        cases = [b'', b'\xef\xbb\xbf{}', b'\xff', b'{}{}', b'{}\v', b'{"a":0,}',
                 b'[1,]', b'{"a":1,"\\u0061":2}', b'"\\x20"', b'"\x00"',
                 b'01', b'+1', b'1.', b'1e', b'-Infinity', b'Infinity', b'NaN',
                 b'{"a":true "b":false}', b'"unterminated', b'[' * 33 + b'0' + b']' * 33,
                 b'-' + b'1' * 4301, b' ' * 1048577]
        for raw in cases:
            with self.subTest(prefix=raw[:60], length=len(raw)):
                self.refusal(self.raw(raw), 'parse')
        for raw in [b'[' * 32 + b'0' + b']' * 32, b'1e999999', b'1.0',
                    b'-' + b'1' * 4300, b'"' + b'[' * 80 + b'"',
                    b'{"x":"\\ud800"}', b'{"x":"\\ud83d\\ude00"}']:
            with self.subTest(prefix=raw[:60], length=len(raw)):
                self.refusal(self.raw(raw), 'schema')

    def test_exact_byte_limit_and_original_hash(self):
        raw = (HERE / 'fixtures/SINGLE-COMMIT.json').read_bytes()
        padded = raw + b' ' * (1048576 - len(raw))
        report = self.success(self.raw(padded))
        self.assertEqual(report['packet_sha256'], hashlib.sha256(padded).hexdigest())
        self.refusal(self.raw(padded + b' '), 'parse')

    def test_escaped_identifiers_and_json_whitespace(self):
        raw = (HERE / 'fixtures/SINGLE-COMMIT.json').read_bytes()
        escaped = b' \t\r\n' + raw.replace(b'op-a', b'op-\\u0061')
        report = self.success(self.raw(escaped))
        self.assertEqual(report['operations'][0]['operation_id'], 'op-a')
        self.assertEqual(report['packet_sha256'], hashlib.sha256(escaped).hexdigest())

    def test_schema_edges_before_binding(self):
        edits = [('amount', True), ('amount', 0), ('amount', -1), ('amount', 2**63),
                 ('amount', 1.0), ('currency', 'eur'), ('currency', 'ÉUR'),
                 ('effect_id', ''), ('effect_id', 'a' * 129), ('effect_id', 'é'),
                 ('status', []), ('dispatch_id', None)]
        for key, value in edits:
            packet = self.base()
            packet['events'][1][key] = value
            packet['events'][0]['seq'] = 4  # binding fault loses to schema
            with self.subTest(key=key, value=value):
                self.refusal(self.raw(json.dumps(packet).encode()), 'schema')
        for field, value in [('admissions', []), ('admissions', [self.base()['admissions'][0]] * 101),
                             ('events', []), ('events', [self.base()['events'][0]] * 1001),
                             ('case_id', None), ('effect_coverage', []), ('schema', 1)]:
            packet = self.base()
            packet[field] = value
            with self.subTest(field=field):
                self.refusal(self.raw(json.dumps(packet).encode()), 'schema')

    def test_amount_boundaries_are_integers(self):
        for amount in [1, 2**63 - 1]:
            packet = self.base()
            packet['events'][1]['amount'] = amount
            self.assertEqual(self.success(self.raw(json.dumps(packet).encode()))['operations'][0]['result'],
                             'ESTABLISHED')

    def test_incomplete_and_unattributed_pending_precedence(self):
        packet = self.base()
        packet['effect_coverage'] = 'incomplete'
        packet['events'][0]['approval_digest'] = None
        packet['events'][1]['status'] = 'pending'
        report = self.success(self.raw(json.dumps(packet).encode()))
        self.assertEqual(report['operations'][0]['reason'], 'incomplete_effect_coverage')
        self.assertEqual(report['unattributed_blocking_effect_ids'], ['e1'])
        packet['effect_coverage'] = 'complete'
        report = self.success(self.raw(json.dumps(packet).encode()))
        self.assertEqual(report['operations'][0]['reason'], 'unattributed_effect_in_scope')
        self.assertEqual(report['operations'][0]['committed_effect_ids'], [])

    def test_all_lifecycle_paths_and_invalid_transitions(self):
        # Explicit paths and literal diagnostic expectations, independent of implementation.
        valid = [(('pending',), [], [], []), (('pending', 'committed'), ['e1'], [], []),
                 (('pending', 'failed'), [], [], []),
                 (('pending', 'committed', 'reversed'), ['e1'], ['e1'], []),
                 (('pending', 'reversed'), ['e1'], ['e1'], ['e1']),
                 (('committed',), ['e1'], [], []),
                 (('committed', 'reversed'), ['e1'], ['e1'], []),
                 (('failed',), [], [], []), (('reversed',), ['e1'], ['e1'], ['e1'])]
        invalid = [('committed', 'pending'), ('failed', 'pending'), ('failed', 'reversed'),
                   ('reversed', 'committed'), ('pending', 'committed', 'failed')]
        for path, committed, reversed_ids, unobserved in valid:
            packet = self.packet_with_path(path)
            row = self.success(self.raw(json.dumps(packet).encode()))['operations'][0]
            self.assertEqual(row['committed_effect_ids'], committed)
            self.assertEqual(row['reversed_effect_ids'], reversed_ids)
            self.assertEqual(row['commit_unobserved_effect_ids'], unobserved)
        for path in invalid:
            with self.subTest(path=path):
                self.refusal(self.raw(json.dumps(self.packet_with_path(path)).encode()), 'binding')

    def packet_with_path(self, statuses):
        packet = self.base()
        observation = packet['events'][1]
        close = packet['events'][2]
        packet['events'] = [packet['events'][0]]
        for index, status in enumerate(statuses, 2):
            packet['events'].append(dict(observation, id=f'o{index}', seq=index, status=status))
        packet['events'].append(dict(close, seq=len(statuses) + 2))
        return packet


if __name__ == '__main__':
    unittest.main()
