"""Synthetic format checks only: these bytes are not a CA measurement."""
import copy
import hashlib
import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from evidence_io import Refused, Store

try:
    import verify_baseline as subject
except ImportError:
    subject = None


def encoded(value):
    return (json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + '\n').encode()


def digest(raw):
    return 'sha256:' + hashlib.sha256(raw).hexdigest()


class BaselinePreparation(unittest.TestCase):
    def setUp(self):
        self.assertIsNotNone(subject, 'baseline verifier preparation absent (setup RED)')
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name).resolve()
        self.packet = b'{"case_id":"tiny","source_class":"synthetic_fixture","custody":"synthetic-simulator","effect_coverage":"complete"}'
        self.oracle = {'operations': [{'operation_id': 'a', 'result': 'ESTABLISHED',
            'reason': 'at_most_one_committed', 'committed_effect_ids': ['e'], 'late_effect_ids': [],
            'commit_unobserved_effect_ids': [], 'reversed_effect_ids': [], 'workflow_keys': ['k'],
            'key_hint_effect_ids': []}], 'epoch': 'closed', 'closure_contradicted': False,
            'unattributed_blocking_effect_ids': []}
        self.report = dict(json.loads(self.packet), **self.oracle,
            schema='refund.c2-report.v0', packet_sha256=hashlib.sha256(self.packet).hexdigest(),
            evidence_basis='fixture_declared', non_claims=['no-live-execution', 'no-provider-semantics',
            'no-exactly-once', 'no-refund-safety', 'no-custody-authentication',
            'no-runtime-coverage-proof', 'no-adequacy-score'])
        self.raw = encoded(self.report)
        self.source = digest(b'source')
        self.backend = digest(b'backend')
        self.dispatch = {'schema': 'corpus-adequacy.invocation-dispatch.v0',
            'backend_sha256': self.backend, 'consumption_sha256': None,
            'execution_profile': 'trusted-local', 'invocation_id': 'inv-1', 'ordinal': 0,
            'session': 'session-1', 'source_sha256': self.source,
            'step_id': 'step-0001', 'vector_id': 'tiny'}
        self.evidence = {'schema': 'corpus-adequacy.invocation-evidence.v0',
            'abnormal': None, 'dispatch_sha256': digest(encoded(self.dispatch)),
            'invocation_id': 'inv-1', 'returncode': 0, 'selector_presence': [True, True],
            'source_sha256': self.source, 'stderr_sha256': digest(b''), 'stderr_size': 0,
            'stdout_sha256': digest(self.raw), 'stdout_size': len(self.raw),
            'step_id': 'step-0001', 'vector_id': 'tiny'}
        self.slot = {'vector_id': 'tiny', 'state': 'observed',
            'outcome': [self.oracle[k] for k in ('operations', 'epoch', 'closure_contradicted',
                                               'unattributed_blocking_effect_ids')],
            'diagnostic': None, 'selector_presence': {'outcome': True, 'diagnostic': True},
            'reason': None, 'evidence_sha256': None,
            'receipt': {'invocation_id': 'inv-1', 'step_id': 'step-0001', 'vector_id': 'tiny',
                'source_sha256': self.source, 'raw_sha256': digest(self.raw),
                'raw_size': len(self.raw), 'evidence_sha256': digest(encoded(self.evidence))}}

    def invoke(self):
        return subject.invocation(self.slot, encoded(self.dispatch), encoded(self.evidence),
            self.raw, b'', self.packet, self.oracle, self.source, self.backend, 'session-1', 0)

    def rebind(self):
        self.evidence['dispatch_sha256'] = digest(encoded(self.dispatch))
        self.slot['receipt']['evidence_sha256'] = digest(encoded(self.evidence))

    def test_literal_report_and_invocation(self):
        self.assertEqual(subject.report(self.raw, self.packet, self.oracle), self.report)
        self.assertEqual(self.invoke(), self.report)

    def test_full_report_and_bool_identity(self):
        for field, value in [('closure_contradicted', 0), ('epoch', 'open'),
                             ('non_claims', []), ('packet_sha256', '0' * 64),
                             ('overall_score', 1)]:
            changed = dict(self.report, **{field: value})
            with self.subTest(field=field), self.assertRaises(Refused):
                subject.report(encoded(changed), self.packet, self.oracle)
        changed = copy.deepcopy(self.report)
        changed['operations'][0]['workflow_keys'] = []
        with self.assertRaises(Refused):
            subject.report(encoded(changed), self.packet, self.oracle)

    def test_dispatch_identity_even_after_digest_rebinding(self):
        for field, value in [('invocation_id', 'different'), ('ordinal', False),
            ('vector_id', 'other'), ('consumption_sha256', digest(b'admit')),
            ('source_sha256', digest(b'other')), ('session', 'other')]:
            original = copy.deepcopy(self.dispatch)
            self.dispatch[field] = value
            self.rebind()
            with self.subTest(field=field), self.assertRaises(Refused):
                self.invoke()
            self.dispatch = original
            self.rebind()

    def test_receipt_and_selected_root_tampering(self):
        for field, value in [('returncode', False), ('stderr_size', True),
                             ('selector_presence', [1, True]), ('invocation_id', 'different')]:
            original = copy.deepcopy(self.evidence)
            self.evidence[field] = value
            self.rebind()
            with self.subTest(field=field), self.assertRaises(Refused):
                self.invoke()
            self.evidence = original
            self.rebind()
        self.slot['outcome'][2] = 0
        with self.assertRaises(Refused):
            self.invoke()

    def test_native_stop_and_no_admission_close(self):
        blob = encoded({'schema': 'corpus-adequacy.observation-operator-stop.v0',
            'reason': 'operator-refused', 'stop_before': 'control',
            'context_sha256': digest(b'context'), 'vector_ids': ['tiny']})
        stop = digest(blob)
        slots = [{'vector_id': 'tiny', 'state': 'not_run', 'outcome': None, 'diagnostic': None,
            'selector_presence': None, 'receipt': None, 'reason': 'operator-refused',
            'evidence_sha256': stop}]
        steps = []
        for i, state in enumerate(['complete', 'complete', 'stopped', 'not_run']):
            steps.append({'step_id': f'step-{i:04d}', 'state': state,
                'source_sha256': self.source if i < 2 else None,
                'application': 'not_applicable' if i < 2 else 'not_run',
                'anchor_hits': None, 'build_state': 'succeeded' if i < 2 else 'not_run',
                'restored': True, 'preflight': None,
                'failure': {'reason': 'operator-refused', 'evidence_sha256': stop} if i == 2 else None,
                'slots': [] if i == 0 else [self.slot] if i == 1 else copy.deepcopy(slots)})
        schedule = [{'step_id': f'step-{i:04d}', 'kind': kind,
            'group': None if i == 0 else 'c2', 'label': None if i < 2 else kind,
            'control_polarity': 'positive' if i == 2 else None,
            'vector_ids': [] if i == 0 else ['tiny'],
            'mutation_sha256': None if i < 2 else digest(kind.encode())}
            for i, kind in enumerate(['build', 'baseline', 'control', 'ordinary'])]
        bindings = {'source_sha256': self.source, 'context_sha256': digest(b'context')}
        cleanup = encoded({'restored': True, 'isolated_tree_removed': True})
        blobs = {stop: blob, digest(cleanup): cleanup}
        prefix = {'schema': 'corpus-adequacy.execution-observation-prefix.v0',
            'session': 'session-1', 'phase': 'stopped', 'bindings': bindings,
            'schedule': schedule, 'steps': steps,
            'cleanup': {'restored': True, 'isolated_tree_removed': True,
                        'evidence_sha256': digest(cleanup)},
            'closure': {'reason': 'operator-refused', 'stop_step': 'step-0002',
                        'prefix_sha256': None, 'admission_sha256': None, 'consumption_sha256': None},
            'non_claims': ['no-adequacy-score', 'no-policy-authentication', 'no-global-replay-prevention']}
        def check(doc, admitted=False):
            raw = encoded(doc)
            final = copy.deepcopy(doc)
            final['schema'] = 'corpus-adequacy.execution-observation.v0'
            final['closure']['prefix_sha256'] = digest(raw)
            if admitted:
                final['closure']['admission_sha256'] = digest(b'bad')
            return subject.observation(raw, encoded(final), bindings, schedule, blobs.__getitem__)
        self.assertEqual(check(prefix), prefix)
        with self.assertRaises(Refused):
            check(prefix, admitted=True)
        for index, field, value in [(2, 'application', 'applied'), (3, 'state', 'complete'),
                                    (1, 'restored', 1), (0, 'source_sha256', None)]:
            changed = copy.deepcopy(prefix)
            changed['steps'][index][field] = value
            with self.subTest(index=index, field=field), self.assertRaises(Refused):
                check(changed)
        changed = copy.deepcopy(prefix)
        changed['steps'][3]['slots'][0]['receipt'] = self.slot['receipt']
        with self.assertRaises(Refused):
            check(changed)

    def test_inventory_exactness_and_mandatory_pair(self):
        files = {'run/operator/result.json': b'{"status":"baseline-matches-oracle"}',
                 'run/operator/provenance.json': b'{"attempt":{"state":"persistence-failed"}}'}
        for name, raw in files.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        inventory = {'schema': 'refund.c2-baseline-retention.v0', 'stores': ['run'],
            'files': [{'store': 'run', 'path': name[4:], 'size': len(raw),
                       'sha256': hashlib.sha256(raw).hexdigest()} for name, raw in files.items()],
            'total_file_bytes': sum(map(len, files.values())),
            'finalization': {'state': 'complete', 'reason': None}}
        (self.root / 'inventory.json').write_bytes(encoded(inventory))
        with Store(self.root) as store:
            retained = subject.Retained(store)
            with self.assertRaises(Refused):
                retained.operator_pair()
        (self.root / 'extra').write_bytes(b'extra')
        with Store(self.root) as store, self.assertRaises(Refused):
            subject.Retained(store)
        (self.root / 'extra').unlink()
        (self.root / 'run/operator/provenance.json').unlink()
        with Store(self.root) as store, self.assertRaises(Refused):
            subject.Retained(store)

    def retained(self, files):
        for name, raw in files.items():
            target = self.root / name
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(raw)
        inventory = {'schema': 'refund.c2-baseline-retention.v0', 'stores': ['run'],
            'files': [{'store': 'run', 'path': name[4:], 'size': len(raw),
                       'sha256': hashlib.sha256(raw).hexdigest()} for name, raw in sorted(files.items())],
            'total_file_bytes': sum(map(len, files.values())),
            'finalization': {'state': 'complete', 'reason': None}}
        (self.root / 'inventory.json').write_bytes(encoded(inventory))
        store = Store(self.root)
        self.addCleanup(store.close)
        return subject.Retained(store)

    def test_bound_pair_failure_dispositions(self):
        result = encoded({'status': 'baseline-matches-oracle'})
        provenance = {'schema': 'refund.c2-baseline-provenance.v0',
            'result_sha256': hashlib.sha256(result).hexdigest(),
            'attempt': {'state': 'closed', 'detail': None, 'source_bound': True,
                        'judged_status': 'baseline-matches-oracle', 'persistence': 'provenance-written'}}
        files = {'run/operator/result.json': result,
                 'run/operator/provenance.json': encoded(provenance)}
        self.assertEqual(self.retained(files).operator_pair()[0], {'status': 'baseline-matches-oracle'})
        for name in ['persistence-failure.json', 'result.pending.json']:
            with self.subTest(marker=name):
                files['run/operator/' + name] = result
                with self.assertRaises(Refused):
                    self.retained(files).operator_pair()
                del files['run/operator/' + name]
                (self.root / 'run/operator' / name).unlink()
        for field, value in [('state', 'unresolved'), ('source_bound', 1), ('persistence', 'pending')]:
            changed = copy.deepcopy(provenance)
            changed['attempt'][field] = value
            files['run/operator/provenance.json'] = encoded(changed)
            with self.subTest(field=field), self.assertRaises(Refused):
                self.retained(files).operator_pair()

    def test_journal_set_and_wrong_session_directory(self):
        receipt_raw, dispatch_raw = encoded(self.evidence), encoded(self.dispatch)
        files = {'run/prefix/session-1/receipt-000000.json': receipt_raw,
                 'run/prefix/session-1/dispatch-000000.json': dispatch_raw,
                 'run/prefix/session-1/call-000000.intent.json': encoded({'dispatch_sha256': digest(dispatch_raw)})}
        for raw in (receipt_raw, dispatch_raw, self.raw, b''):
            files['run/prefix/session-1/blobs/' + hashlib.sha256(raw).hexdigest()] = raw
        prefix = {'session': 'session-1', 'bindings': {'source_sha256': self.source,
            'backend_sha256': self.backend}, 'steps': [{}, {'slots': [self.slot]}]}
        frozen = {'packets': {'tiny': self.packet}, 'oracle': {'tiny': self.oracle}}
        reports, receipts = subject.receipt_set(self.retained(files), prefix, frozen)
        self.assertEqual(reports, {'tiny': self.report})
        result = {'schema': 'refund.c2-baseline-result.v0', 'status': 'baseline-matches-oracle',
            'stop': {'reason': 'operator-refused', 'shape': 'healthy-stop'},
            'non_claims': self.report['non_claims'], 'cases': [{'case_id': 'tiny', 'status': 'matches-oracle',
            'operations': [['a', 'ESTABLISHED', 'at_most_one_committed']], 'differences': [], 'detail': None,
            'invocation': {'returncode': 0, 'abnormal': None, 'stdout_size': len(self.raw), 'stderr_size': 0}}]}
        subject.success_result(result, reports, receipts)
        result['cases'][0]['operations'] = []
        with self.assertRaises(Refused):
            subject.success_result(result, reports, receipts)
        original = dict(files)
        for name in list(files):
            if '/blobs/' not in name:
                raw = files.pop(name)
                (self.root / name).unlink()
                files[name.replace('/session-1/', '/wrong-session/')] = raw
        with self.assertRaises(Refused):
            subject.receipt_set(self.retained(files), prefix, frozen)
        for name in list(files):
            if '/wrong-session/' in name:
                (self.root / name).unlink()
        files = original
        files.pop('run/prefix/session-1/call-000000.intent.json')
        with self.assertRaises(Refused):
            subject.receipt_set(self.retained(files), prefix, frozen)

    def test_frozen_twenty_packets_and_twenty_five_operations(self):
        with Store(Path(__file__).parent / 'record/c2') as store:
            frozen = subject.frozen_inputs(store)
        self.assertEqual(len(frozen['oracle']), 20)
        self.assertEqual(sum(len(x['operations']) for x in frozen['oracle'].values()), 25)

    def test_cli_pending_never_emits_accepted_record(self):
        result = subprocess.run([sys.executable, str(Path(__file__).with_name('verify_baseline.py')),
            '--retained', str(self.root)], capture_output=True, check=False)
        self.assertEqual(result.returncode, 2)
        self.assertEqual(result.stdout, b'')
        self.assertTrue(result.stderr.startswith(b'refused: pending'), result.stderr)


if __name__ == '__main__':
    unittest.main()
