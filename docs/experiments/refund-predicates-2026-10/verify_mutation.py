"""Read-only verifier for one pinned retained mutation run; never executes subjects.

The producer consumer was inspected for format understanding. This implementation
imports neither that consumer nor CA and independently derives projections from
all raw reports. Native serialization rules follow the pinned public CA codecs.
"""
import argparse
import hashlib
import json
import re
import sys
from pathlib import Path

from evidence_io import Refused, Store, loads, need, open_retained

INVENTORY = '3e536b335a9532f388b3e61646b4f16d33be45391e56972046cce05f115a3e21'
RESULT = 'c26f5b562e2963a469540237591ad3f3ca24e8a1ef0027164efbe50f536b9625'
PATCHES = ('F1', 'F2', 'F3', 'F4a', 'F4b', 'F5', 'F6')
AXES = ('definite_support', 'uncertainty_calibration')
NON_CLAIMS = ['no-live-execution', 'no-runtime-coverage-proof', 'no-provider-authentication',
              'no-effect-truth', 'no-C2', 'no-adequacy-score']


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def digest(raw):
    return 'sha256:' + sha(raw)


def native(value):
    # CA canonical_bytes is indented, sorted, ASCII JSON with one final newline.
    return (json.dumps(value, sort_keys=True, indent=2, ensure_ascii=True, allow_nan=False) + '\n').encode()


def closed(value, keys):
    need(type(value) is dict and set(value) == set(keys.split()), 'closed evidence shape')


def hash_text(value):
    need(type(value) is str and re.fullmatch(r'sha256:[0-9a-f]{64}', value) is not None, 'digest syntax')
    return value[7:]


def check_receipt(r):
    closed(r, 'schema abnormal dispatch_sha256 invocation_id returncode selector_presence source_sha256 '
              'stderr_sha256 stderr_size stdout_sha256 stdout_size step_id vector_id')
    need(r['schema'] == 'corpus-adequacy.invocation-evidence.v0', 'receipt schema')
    need(type(r['returncode']) is int and r['returncode'] == 0 and r['abnormal'] is None,
         'process unsuccessful')
    need(type(r['stderr_size']) is int and r['stderr_size'] == 0, 'stderr present')
    need(type(r['stdout_size']) is int and 0 <= r['stdout_size'] <= 4 * 1024 * 1024, 'stdout size')
    need(r['selector_presence'] == [True, True] and all(type(v) is bool for v in r['selector_presence']),
         'selector absent')
    for field in ('source_sha256', 'dispatch_sha256', 'stdout_sha256', 'stderr_sha256'):
        hash_text(r[field])
    for field in ('invocation_id', 'step_id', 'vector_id'):
        need(type(r[field]) is str and 0 < len(r[field]) <= 128, 'receipt identity')


def report(raw, packet, packet_raw, oracle):
    value = loads(raw)
    closed(value, 'schema case_id operation_id packet_sha256 source_class evidence_basis delivery_coverage claims non_claims')
    need(value['schema'] == 'refund.c3-report.v0' and value['non_claims'] == NON_CLAIMS, 'report contract')
    need(value['source_class'] == 'synthetic_fixture' and value['evidence_basis'] == 'fixture_declared', 'report basis')
    for field in ('case_id', 'operation_id', 'delivery_coverage'):
        need(value[field] == packet[field], 'report identity')
    need(value['packet_sha256'] == sha(packet_raw), 'packet identity')
    rows = value['claims']
    need(type(rows) is list and len(rows) == len(oracle), 'report row count')
    for actual, expected in zip(rows, oracle):
        closed(actual, 'report_id report_seq reported_outcome eligible_delivery_ids eligible_result_ids conflict definite_support uncertainty_calibration')
        for field in ('report_id', 'report_seq', 'reported_outcome'):
            need(type(actual[field]) is type(expected[field]) and actual[field] == expected[field], 'claim identity')
        need(type(actual['conflict']) is bool, 'conflict type')
        for field in ('eligible_delivery_ids', 'eligible_result_ids'):
            need(type(actual[field]) is list and all(type(x) is str for x in actual[field]), 'eligible ids type')
        for axis in AXES:
            claim = actual[axis]
            closed(claim, 'applicable result reason')
            need(type(claim['applicable']) is bool and claim['result'] in
                 ('ESTABLISHED', 'CONTRADICTED', 'NOT_ESTABLISHED', None), 'claim value')
            need(claim['reason'] in ('terminal_support', 'unresolved_support', 'unsupported_definite_report',
                 'unnecessarily_unknown', 'conflicting_terminal_evidence', 'incomplete_visibility',
                 'contrary_terminal_evidence', 'not_definite_report', 'not_unknown_report'), 'claim reason')
    return rows


def compare_claims(observed, oracle):
    need(set(observed) == set(oracle), 'incomplete case coverage')
    changed, axes, extras = [], [], []
    for case in sorted(oracle):
        need(len(observed[case]) == len(oracle[case]), 'row count')
        if observed[case] != oracle[case]:
            changed.append(case)
        for got, expected in zip(observed[case], oracle[case]):
            need(got['report_id'] == expected['report_id'], 'row order')
            ident = {'case_id': case, 'report_id': expected['report_id']}
            for axis in AXES:
                prior = [expected[axis]['applicable'], expected[axis]['result']]
                after = [got[axis]['applicable'], got[axis]['result']]
                if prior != after:
                    axes.append(dict(ident, axis=axis, oracle=prior, observed=after))
                if expected[axis]['reason'] != got[axis]['reason']:
                    extras.append(dict(ident, field=axis + '.reason', oracle=expected[axis]['reason'], observed=got[axis]['reason']))
            for field in ('conflict', 'eligible_delivery_ids', 'eligible_result_ids'):
                if got[field] != expected[field]:
                    extras.append(dict(ident, field=field, oracle=expected[field], observed=got[field]))
    return changed, axes, extras


def source_digest(raw):
    name = b'reader.py'
    framed = b'corpus-adequacy.observation-sources.v0\n' + str(len(name)).encode() + b'\n' + name
    return digest(framed + str(len(raw)).encode() + b'\n' + raw)


class Evidence:
    def __init__(self, store):
        self.store = store
        self.blobs = {}
        self.receipts = {}
        self.intents = set()
        self.used = set()
        inv_raw = store.read('inventory.json')
        need(sha(inv_raw) == INVENTORY, 'not the pinned original inventory')
        inv = loads(inv_raw)
        closed(inv, 'schema stores files total_file_bytes')
        need(inv['schema'] == 'refund.c3-mutation-retention.v0' and inv['stores'] == ['run', 'ledger'], 'inventory schema')
        expected = set()
        total = 0
        for entry in inv['files']:
            closed(entry, 'store path size sha256')
            need(entry['store'] in ('run', 'ledger') and type(entry['size']) is int and entry['size'] >= 0,
                 'inventory field')
            name = entry['store'] + '/' + entry['path']
            need(name not in expected, 'duplicate inventory path')
            expected.add(name)
            raw = store.read(name)
            need(len(raw) == entry['size'] and sha(raw) == entry['sha256'], 'inventory bytes differ')
            total += len(raw)
            if '/blobs/' in name:
                h = name.rsplit('/', 1)[1]
                need(h == sha(raw), 'blob content address')
                self.blobs.setdefault('sha256:' + h, []).append(name)
            if re.search(r'/receipt-[0-9]{6}\.json$', name):
                receipt = loads(raw)
                check_receipt(receipt)
                rid = receipt['invocation_id']
                need(rid not in self.receipts, 'duplicate invocation receipt')
                self.receipts[rid] = (receipt, raw)
            if re.search(r'/call-[0-9]{6}\.intent\.json$', name):
                intent = loads(raw)
                closed(intent, 'dispatch_sha256')
                need(intent['dispatch_sha256'] not in self.intents, 'duplicate dispatch intent')
                self.intents.add(intent['dispatch_sha256'])
        need(set(store.names) == expected | {'inventory.json'}, 'extra or missing retained files')
        need(total == inv['total_file_bytes'] == 11895172 and len(expected) == 7098, 'retained count/size')
        need(len(self.receipts) == len(self.intents) == 900, 'incomplete receipt inventory')
        self.inventory_files = len(expected)
        self.blob_files = sum(len(v) for v in self.blobs.values())
        need(self.blob_files == 3961, 'blob inventory count')

    def blob(self, value):
        hash_text(value)
        need(value in self.blobs, 'missing referenced blob')
        raw = self.store.read(self.blobs[value][0])
        need(digest(raw) == value, 'blob changed')
        return raw

    def doc(self, value):
        return loads(self.blob(value))

    def slot(self, slot, step, case, packet, packet_raw, oracle, source, session):
        closed(slot, 'vector_id state outcome diagnostic selector_presence receipt reason evidence_sha256')
        need(slot['vector_id'] == case and slot['state'] == 'observed' and slot['reason'] is None,
             'slot not observed')
        need(slot['selector_presence'] == {'diagnostic': True, 'outcome': True}, 'slot selector')
        r = slot['receipt']
        closed(r, 'invocation_id step_id vector_id source_sha256 raw_sha256 raw_size evidence_sha256')
        need(r['invocation_id'] in self.receipts and r['invocation_id'] not in self.used, 'receipt reuse/missing')
        self.used.add(r['invocation_id'])
        evidence, receipt_raw = self.receipts[r['invocation_id']]
        need(self.blob(r['evidence_sha256']) == receipt_raw, 'receipt bytes binding')
        for field, expected in [('step_id', step['step_id']), ('vector_id', case), ('source_sha256', source)]:
            need(r[field] == evidence[field] == expected, 'receipt slot binding')
        need(r['raw_sha256'] == evidence['stdout_sha256'] and r['raw_size'] == evidence['stdout_size'], 'stdout binding')
        need(self.blob(evidence['stderr_sha256']) == b'', 'stderr bytes')
        raw = self.blob(r['raw_sha256'])
        need(len(raw) == r['raw_size'], 'raw size binding')
        dispatch = self.doc(evidence['dispatch_sha256'])
        closed(dispatch, 'schema backend_sha256 consumption_sha256 execution_profile invocation_id ordinal session source_sha256 step_id vector_id')
        need(dispatch['schema'] == 'corpus-adequacy.invocation-dispatch.v0' and evidence['dispatch_sha256'] in self.intents,
             'dispatch intent binding')
        for field in ('invocation_id', 'step_id', 'vector_id', 'source_sha256'):
            need(dispatch[field] == evidence[field], 'dispatch receipt binding')
        need(dispatch['session'] == session['session'] and dispatch['backend_sha256'] == session['bindings']['backend_sha256']
             and dispatch['execution_profile'] == 'trusted-local', 'dispatch session binding')
        if step['step_id'] == 'step-0004':
            need(dispatch['consumption_sha256'] == session['closure']['consumption_sha256'], 'dispatch consumption')
        else:
            need(dispatch['consumption_sha256'] is None, 'unexpected consumption')
        rows = report(raw, packet, packet_raw, oracle)
        need(slot['outcome'] == [rows], 'selected outcome differs from raw')
        return rows


def target_rows(targets, axes):
    movement = {(x['case_id'], x['report_id'], x['axis']): x for x in axes}
    rows = []
    for target in targets:
        key = (target['case_id'], target['report_id'], target['axis'])
        need(key in movement, 'predicted target did not move')
        actual = movement[key]
        need(actual['oracle'] == target['oracle'] and actual['observed'] == target['predicted'], 'target prediction differs')
        rows.append(dict(target, observed=actual['observed'], outcome='as-predicted'))
    return rows


def verify(retained, mutation_inputs, c3_inputs):
    with open_retained(retained) as store, Store(mutation_inputs) as inputs, Store(c3_inputs) as c3:
        e = Evidence(store)
        pins = inputs.json('SOURCE-PINS.json')
        oracle = c3.json('expectations.json')
        targets = inputs.json('targets.json')['patches']
        patches = {p['id']: p for p in inputs.json('patches.json')['patches']}
        freeze = inputs.json('patch-freeze.json')['patches']
        original = c3.read('reader.py')
        need(sha(original) == pins['application']['sha256']['reader.py'], 'measured reader pin')
        need(sha(c3.read('expectations.json')) == pins['application']['sha256']['expectations.json'], 'oracle pin')
        need(len(oracle) == 30 and sum(map(len, oracle.values())) == 35, 'oracle size')
        source = {'baseline': source_digest(original)}
        for pid, patch in patches.items():
            anchor, replacement = patch['anchor'].encode(), patch['replacement'].encode()
            need(original.count(anchor) == 1, 'patch anchor multiplicity')
            mutated = original.replace(anchor, replacement)
            need(sha(mutated) == freeze[pid]['mutated_sha256'] and sha(original) == freeze[pid]['original_sha256'], 'patch freeze mismatch')
            need(sha(inputs.read('diffs/' + pid + '.diff')) == freeze[pid]['diff_sha256'], 'patch diff mismatch')
            source[pid] = source_digest(mutated)
        packets = {case: c3.read('fixtures/' + case + '.json') for case in oracle}
        docs = {case: loads(raw) for case, raw in packets.items()}
        for name, expected in pins['instrument']['files'].items():
            need(sha(store.read('run/export/' + name)) == expected, 'instrument source bytes')
        rows = []
        baseline_outputs = []
        target_counts = {}
        for label in ('B1', 'B2') + PATCHES:
            final_names = [n for n in store.names if n.startswith('run/sessions/' + label + '/') and n.endswith('/final.json')]
            need(len(final_names) == 1, 'session final multiplicity')
            final_raw = store.read(final_names[0])
            session = loads(final_raw)
            closed(session, 'bindings cleanup closure non_claims phase schedule schema session steps')
            need(session['schema'] == 'corpus-adequacy.execution-observation.v0', 'session schema')
            need(session['phase'] == ('stopped' if label in ('B1', 'B2') else 'complete'), 'session not completed')
            need(session['cleanup']['restored'] is True and session['cleanup']['isolated_tree_removed'] is True, 'unsafe cleanup')
            cleanup = e.doc(session['cleanup']['evidence_sha256'])
            need(cleanup['restored'] is True and cleanup['isolated_tree_removed'] is True, 'cleanup evidence')
            binding = session['bindings']
            need(binding['source_sha256'] == source['baseline'] and binding['tool_source_state'] == 'unresolved'
                 and binding['tool_commit'] is None, 'source binding state')
            context_raw = store.read('run/operator/' + label + '/context.bin')
            need(digest(context_raw) == binding['context_sha256'], 'context binding')
            need(store.read('run/subjects/' + label + '/reader.py') == original, 'original subject bytes')
            manifest = store.read('run/subjects/' + label + '/manifest.json')
            need(digest(manifest) == binding['manifest_sha256'], 'manifest binding')
            for case, raw in packets.items():
                need(store.read('run/subjects/' + label + '/fixtures/' + case + '.json') == raw, 'subject fixture binding')
            need(len(session['steps']) == len(session['schedule']) == 5, 'schedule length')
            outputs = {}
            for index, (declaration, step) in enumerate(zip(session['schedule'], session['steps'])):
                need(declaration['step_id'] == step['step_id'] == 'step-' + str(index).zfill(4), 'schedule order')
                if index == 0:
                    need(step['state'] == 'complete' and step['build_state'] == 'succeeded' and step['slots'] == [], 'empty build')
                    continue
                if label in ('B1', 'B2') and index > 1:
                    need(step['state'] == ('stopped' if index == 2 else 'not_run') and all(s['state'] == 'not_run' for s in step['slots']), 'baseline-only suffix executed')
                    continue
                pid = ('baseline', 'POS', 'INERT', label)[index - 1]
                need(step['state'] == 'complete' and step['build_state'] == 'succeeded' and step['restored'] is True
                     and step['failure'] is None and step['source_sha256'] == source[pid], 'step failed/source mismatch')
                if index > 1:
                    need(step['application'] == 'applied' and step['anchor_hits'] == 1, 'patch not applied exactly once')
                    need(declaration['label'] == patches[pid]['label'], 'patch label')
                ids = declaration['vector_ids']
                need(len(ids) == len(set(ids)) == len(oracle) and set(ids) == set(oracle), 'schedule vector coverage')
                need([s['vector_id'] for s in step['slots']] == ids, 'slot coverage/order')
                outputs[pid] = {case: e.slot(slot, step, case, docs[case], packets[case], oracle[case], source[pid], session)
                                for case, slot in zip(ids, step['slots'])}
            need(outputs['baseline'] == oracle, 'baseline differs')
            baseline_outputs.append(outputs['baseline'])
            if label in ('B1', 'B2'):
                need(session['closure']['reason'] == 'operator-refused' and session['closure']['stop_step'] == 'step-0002', 'baseline closure')
                continue
            need(outputs['INERT'] == oracle, 'inert control differs')
            pos_changes, pos_axes, _ = compare_claims(outputs['POS'], oracle)
            pos_targets = target_rows(targets['POS']['targets'], pos_axes)
            need(bool(pos_changes), 'positive control unchanged')
            verify_admission(e, label, session, context_raw, pos_changes, pos_targets)
            changed, axes, extras = compare_claims(outputs[label], oracle)
            wanted = targets[label]['targets']
            observed_targets = target_rows(wanted, axes)
            expected_keys = {(t['case_id'], t['report_id'], t['axis']) for t in wanted}
            unexpected = [a for a in axes if (a['case_id'], a['report_id'], a['axis']) not in expected_keys]
            need(not unexpected, 'unexpected axis movement')
            target_counts[label] = len(observed_targets)
            rows.append({'patch': label, 'status': 'targeted-detected', 'failure': None,
                         'partial_observations': [], 'targets': observed_targets, 'unexpected': unexpected,
                         'extra': extras, 'full_claims_changed': changed,
                         'vector_status': {case: 'observed' for case in sorted(oracle)}})
        need(len(e.used) == len(e.receipts) == 900, 'unconsumed or missing receipt')
        need({r['dispatch_sha256'] for r, _ in e.receipts.values()} == e.intents, 'receipt intent coverage')
        result = {'schema': 'refund.c3-mutation-result.v0',
                  'preflight': {'ok': True, 'sessions': [{'changed': [], 'incomplete': False}] * 2,
                                'states': {'B1': 'closed', 'B2': 'closed'}}, 'rows': rows}
        original_result = store.read('run/operator/result.json')
        need(sha(original_result) == RESULT and original_result == inputs.read('result.json'), 'result byte binding')
        need(result == loads(original_result), 'recomputed result differs')
        verify_budget(store.json('run/operator/provenance.json'))
        return {'schema': 'refund.offline-mutation-verification.v0', 'inventory_files': e.inventory_files,
                'blob_files': e.blob_files, 'receipts': len(e.receipts), 'dispatches': len(e.intents),
                'target_counts': target_counts, 'unexpected_axes': 0, 'result_sha256': sha(original_result),
                'execution': 'not-performed', 'custody_authenticated': False}


def verify_admission(e, label, session, context_raw, pos_changes, pos_targets):
    store = e.store
    prefix_raw = e.blob(session['closure']['prefix_sha256'])
    prefix = loads(prefix_raw)
    need(prefix['phase'] == 'awaiting_admission' and prefix['session'] == session['session'], 'prefix identity')
    need(prefix['bindings'] == session['bindings'] and prefix['schedule'] == session['schedule'], 'prefix binding')
    need(prefix['steps'][:4] == session['steps'][:4], 'prefix steps differ')
    admission_raw = store.read('run/operator/' + label + '/admission.json')
    admission = loads(admission_raw)
    need(digest(admission_raw) == session['closure']['admission_sha256'], 'admission digest')
    need(admission['decision'] == 'allow' and admission['reasons'] == [] and admission['next_step'] == 'step-0004', 'admission decision')
    for field in ('session',):
        need(admission[field] == session[field], 'admission session')
    for field in ('policy_identity', 'interpreter_identity', 'context_sha256'):
        need(admission[field] == session['bindings'][field], 'admission binding')
    need(admission['prefix_sha256'] == digest(prefix_raw) and admission['context_sha256'] == digest(context_raw), 'admission prefix/context')
    unsigned = dict(admission)
    unsigned.pop('decision_binding_sha256')
    need(digest(native(unsigned)) == admission['decision_binding_sha256'], 'decision binding')
    gate_raw = store.read('run/operator/' + label + '/gate.json')
    need(digest(gate_raw) == admission['decision_sha256'], 'gate digest')
    gate = loads(gate_raw)
    need(gate['decision'] == 'allow' and gate['reasons'] == [], 'gate refusal')
    need(gate['checks'] == {'baseline_changed': [], 'cleanup_safe': True, 'incomplete': {},
         'inert_changed': [], 'phase': 'awaiting_admission', 'positive_changed': pos_changes,
         'positive_targets': pos_targets}, 'gate differs from derived controls')
    identity = store.json('run/operator/' + label + '/ledger-identity.json')
    need(gate['ledger_identity'] == loads(context_raw)['ledger_identity'] == identity, 'ledger identity binding')
    # Historical paths are compared only as inert metadata, never resolved/opened.
    ledger_names = [n for n in store.names if n.startswith('ledger/' + label + '/') and n.endswith('.consumed.json')]
    need(len(ledger_names) == 1, 'consumption ledger count')
    consumption_raw = store.read(ledger_names[0])
    need(digest(consumption_raw) == session['closure']['consumption_sha256'], 'consumption ledger digest')
    consumption = loads(consumption_raw)
    need(consumption['admission_sha256'] == digest(admission_raw) and consumption['prefix_sha256'] == digest(prefix_raw)
         and consumption['session'] == session['session'] and consumption['nonce'] == admission['nonce'], 'consumption binding')
    completed = store.json(ledger_names[0].replace('.consumed.json', '.consumed.completed.json'))
    need(completed['final_sha256'] == digest(store.read(next(n for n in store.names if n.startswith('run/sessions/' + label + '/') and n.endswith('/final.json')))), 'completed ledger final')


def verify_budget(provenance):
    budget = provenance['budget']
    need(budget['calls_reserved'] == 900 and len(budget['decisions']) == 1252, 'budget evidence count')
    for row in budget['decisions']:
        need(row['ok'] is True and row['limit'] == 268435456, 'budget decision')
        amount = sum(row.get(k, 0) for k in ('used', 'bytes', 'reserve', 'copy_allowance'))
        need(amount <= row['limit'], 'budget exceeded')


def main():
    here = Path(__file__).resolve().parent
    parser = argparse.ArgumentParser(description=__doc__.split('\n')[0])
    parser.add_argument('--retained', type=Path, default=here / 'record/mutation/retained.tar.gz')
    parser.add_argument('--mutation-inputs', type=Path, default=here / 'record/mutation')
    parser.add_argument('--c3-inputs', type=Path, default=here / 'record/c3')
    args = parser.parse_args()
    try:
        result = verify(args.retained, args.mutation_inputs, args.c3_inputs)
    except (Refused, KeyError, TypeError, IndexError, OSError, ValueError) as error:
        print('refused: ' + (str(error) if isinstance(error, Refused) else 'malformed evidence'), file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
