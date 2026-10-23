"""Offline verifier for one pinned retained C2 baseline.

Only stdlib byte checks run here. No subjects, CA modules or historical paths are
executed or opened. The output describes retained bytes, never a new execution or custody proof.
"""
import argparse
import copy
import hashlib
import json
import re
import sys
from pathlib import Path

from evidence_io import Refused, Store, loads, need

SELECTORS = ['operations', 'epoch', 'closure_contradicted', 'unattributed_blocking_effect_ids']
NON_CLAIMS = ['no-live-execution', 'no-provider-semantics', 'no-exactly-once',
              'no-refund-safety', 'no-custody-authentication', 'no-runtime-coverage-proof',
              'no-adequacy-score']
CA_NON_CLAIMS = ['no-adequacy-score', 'no-policy-authentication', 'no-global-replay-prevention']
# Fixed acceptance identities; the CLI never accepts caller-supplied replacements.
ACCEPTED_INVENTORY_SHA256 = 'a3945cff972dede78a5002d8b0a25706d5d1f4ce243c13d2cfe7f03fda166cd3'
ACCEPTED_RESULT_SHA256 = '402cef44fbdb891f870414b7a0e5da14a7ca36422af02c5e7e538ad4cac31f6e'
ACCEPTED_PROVENANCE_SHA256 = '766dc3696740535f08376194ee08d21e3c17b745b042ebbbefaec79dc8749d6a'
CARRIER_MAP_SHA256 = '9df3e0d62de2e795420281ae696a50987aea6f420200beeb3d163dcffb6bbc06'
CARRIER_COMMIT = '6bdeb4b10f3952ccd2fe6f792345f5a0d44efec4'
POLICY = b'refund.c2-baseline: baseline-only stop before control; no admission, ledger or resume; no score'
HERE = Path(__file__).parent
FROZEN_HASHES = {
    'reader.py': '1aa235352d15434e3a6e6630ed96632f189f82bcf8df5ea4b71c5964374dcf89',
    'expectations.json': '0b237c534e883b7e0b909d7c84488abc6bd6bcd6d5ec2e5aae3056afbc46c00b',
    'fixtures.sha256.json': '335a45bf556f7645561c1b60a7a77959edf2aaccb3944f2330a9d62932fcf0fd',
}
SUBJECT_HASHES = {
    'reader.py': FROZEN_HASHES['reader.py'],
    'ca-manifest.json': '9c723c4b76cf003bbcd932ad879cfb0977913dd2fc4793bbf13ab6de0be8de77',
    'ca-vectors.json': '35915f3a6e4f30453ad56037156d1e96b30989e9e20b4d67cf04b747cfd285d5',
}


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def digest(raw):
    return 'sha256:' + sha(raw)


def native(value):
    """Pinned CA observation codec uses UTF-8, sorted indented JSON and newline."""
    return (json.dumps(value, ensure_ascii=False, allow_nan=False, sort_keys=True, indent=2) + '\n').encode()


def same(actual, expected, reason):
    # JSON representation distinguishes bool from int recursively, unlike ==.
    need(native(actual) == native(expected), reason)


def closed(value, keys):
    need(type(value) is dict and set(value) == set(keys.split()), 'closed evidence shape')


def hexdigest(value):
    need(type(value) is str and re.fullmatch(r'[0-9a-f]{64}', value) is not None, 'digest syntax')
    return value


def canonical(raw):
    value = loads(raw)
    need(native(value) == raw, 'noncanonical CA bytes')
    return value


def source_digest(raw):
    return digest(b'corpus-adequacy.observation-sources.v0\n9\nreader.py' +
                  str(len(raw)).encode() + b'\n' + raw)


def frozen_inputs(store):
    raw = {name: store.read(name) for name in FROZEN_HASHES}
    for name, value in raw.items():
        need(sha(value) == FROZEN_HASHES[name], 'frozen input pin differs')
    oracle = loads(raw['expectations.json'])
    hashes = loads(raw['fixtures.sha256.json'])
    packets = {case: store.read('fixtures/' + case + '.json') for case in oracle}
    need(len(oracle) == 20 and sum(len(v['operations']) for v in oracle.values()) == 25,
         'frozen coverage differs')
    for case, packet in packets.items():
        need(sha(packet) == hashes['fixtures/' + case + '.json'], 'fixture differs')
        need(loads(packet)['case_id'] == case, 'fixture identity differs')
    return {'oracle': oracle, 'packets': packets, 'reader': raw['reader.py']}


def subject_inputs(store, frozen):
    """Pin the retained subject, vector order, corpus and declared schedule."""
    raw = {name: store.read('run/subject/' + name) for name in SUBJECT_HASHES}
    for name, value in raw.items():
        need(sha(value) == SUBJECT_HASHES[name], 'subject source/vector pin differs')
    need(raw['reader.py'] == frozen['reader'], 'retained reader differs')
    manifest = loads(raw['ca-manifest.json'])
    vectors = loads(raw['ca-vectors.json'])
    ids = sorted(frozen['oracle'])
    same(vectors, [{'id': case, 'path': 'fixtures/' + case + '.json'} for case in ids], 'vector route differs')
    rows = [{'index_sha256': digest(raw['ca-vectors.json'])}]
    for vector in vectors:
        value = store.read('run/subject/' + vector['path'])
        need(value == frozen['packets'][vector['id']], 'retained fixture differs')
        rows.append({'vector_id': vector['id'], 'path': vector['path'], 'sha256': digest(value)})
    schedule = []
    for index, kind in enumerate(('build', 'baseline', 'control', 'ordinary')):
        mutant = dict(manifest['mutants']['c2'][index - 2]) if index >= 2 else None
        if mutant is not None:
            # Pinned CA load_manifest_bytes normalizes omitted control to false
            # before the native schedule hashes the declaration.
            mutant.setdefault('control', False)
        schedule.append({'step_id': f'step-{index:04d}', 'kind': kind,
            'group': None if index == 0 else 'c2', 'label': mutant['label'] if mutant else None,
            'control_polarity': 'positive' if index == 2 else None,
            'vector_ids': [] if index == 0 else ids,
            'mutation_sha256': digest(native(mutant)) if mutant else None})
    return {'source_sha256': source_digest(raw['reader.py']),
            'manifest_sha256': digest(raw['ca-manifest.json']), 'corpus_sha256': digest(native(rows)),
            'selectors': {'outcome_from': SELECTORS, 'diagnostic_from': None},
            'exit_policy': {'accepted_exit_codes': [0], 'unproved_exit_codes': [2]},
            'execution_profile': 'trusted-local'}, schedule


def report(raw, packet_raw, oracle):
    packet = loads(packet_raw)
    expected = {key: packet[key] for key in ('case_id', 'source_class', 'custody', 'effect_coverage')}
    expected.update(schema='refund.c2-report.v0', packet_sha256=sha(packet_raw),
                    evidence_basis='fixture_declared', non_claims=NON_CLAIMS)
    expected.update(oracle)
    actual = loads(raw)
    same(actual, expected, 'full raw report differs from frozen identity/oracle')
    return actual


def invocation(slot, dispatch_raw, evidence_raw, stdout, stderr, packet_raw, oracle,
               source, backend, session, ordinal):
    dispatch, evidence = canonical(dispatch_raw), canonical(evidence_raw)
    closed(dispatch, 'schema backend_sha256 consumption_sha256 execution_profile invocation_id ordinal '
                     'session source_sha256 step_id vector_id')
    closed(evidence, 'schema abnormal dispatch_sha256 invocation_id returncode selector_presence source_sha256 '
                     'stderr_sha256 stderr_size stdout_sha256 stdout_size step_id vector_id')
    closed(slot, 'vector_id state outcome diagnostic selector_presence receipt reason evidence_sha256')
    receipt = slot['receipt']
    closed(receipt, 'invocation_id step_id vector_id source_sha256 raw_sha256 raw_size evidence_sha256')
    ident = receipt['invocation_id']
    need(type(ident) is str and 0 < len(ident) <= 128, 'invocation identity')
    case = loads(packet_raw)['case_id']
    base = {'invocation_id': ident, 'step_id': 'step-0001', 'vector_id': case, 'source_sha256': source}
    same(dispatch, dict(base, schema='corpus-adequacy.invocation-dispatch.v0',
        backend_sha256=backend, consumption_sha256=None, execution_profile='trusted-local',
        session=session, ordinal=ordinal), 'dispatch invocation identity differs')
    same(evidence, dict(base, schema='corpus-adequacy.invocation-evidence.v0',
        abnormal=None, dispatch_sha256=digest(dispatch_raw), returncode=0, selector_presence=[True, True],
        stderr_sha256=digest(b''), stderr_size=0, stdout_sha256=digest(stdout), stdout_size=len(stdout)),
        'raw receipt differs')
    need(stderr == b'', 'nonempty stderr')
    same(receipt, dict(base, raw_sha256=digest(stdout), raw_size=len(stdout),
                      evidence_sha256=digest(evidence_raw)), 'slot receipt differs')
    value = report(stdout, packet_raw, oracle)
    same(slot, {'vector_id': case, 'state': 'observed', 'outcome': [value[k] for k in SELECTORS],
        'diagnostic': None, 'selector_presence': {'outcome': True, 'diagnostic': True},
        'receipt': receipt, 'reason': None, 'evidence_sha256': None}, 'root projection/slot differs')
    return value


def observation(prefix_raw, final_raw, bindings, schedule, blob):
    prefix, final = canonical(prefix_raw), canonical(final_raw)
    closed(prefix, 'schema session phase bindings schedule steps cleanup closure non_claims')
    need(prefix['schema'] == 'corpus-adequacy.execution-observation-prefix.v0', 'prefix schema')
    need(type(prefix['session']) is str and 0 < len(prefix['session']) <= 128, 'session identity')
    same(prefix['bindings'], bindings, 'observation bindings differ')
    same(prefix['schedule'], schedule, 'schedule differs')
    same(prefix['non_claims'], CA_NON_CLAIMS, 'CA nonclaims differ')
    need(prefix['phase'] == 'stopped', 'prefix not stopped')
    same(prefix['closure'], {'reason': 'operator-refused', 'stop_step': 'step-0002',
        'prefix_sha256': None, 'admission_sha256': None, 'consumption_sha256': None}, 'closure differs')
    cleanup = prefix['cleanup']
    closed(cleanup, 'restored isolated_tree_removed evidence_sha256')
    same({k: cleanup[k] for k in ('restored', 'isolated_tree_removed')},
         {'restored': True, 'isolated_tree_removed': True}, 'cleanup failed')
    same(canonical(blob(cleanup['evidence_sha256'])), {'restored': True, 'isolated_tree_removed': True},
         'cleanup evidence differs')
    steps = prefix['steps']
    need(type(steps) is list and len(steps) == 4, 'step coverage')
    failure = steps[2]['failure']
    closed(failure, 'reason evidence_sha256')
    need(failure['reason'] == 'operator-refused', 'control stop reason')
    stop = failure['evidence_sha256']
    same(canonical(blob(stop)), {'schema': 'corpus-adequacy.observation-operator-stop.v0',
        'reason': 'operator-refused', 'stop_before': 'control',
        'context_sha256': bindings['context_sha256'], 'vector_ids': schedule[1]['vector_ids']},
        'stop evidence differs')
    for index, step in enumerate(steps):
        closed(step, 'step_id state source_sha256 application anchor_hits build_state restored preflight failure slots')
        slots = step['slots']
        need(type(slots) is list, 'slots shape')
        same([s['vector_id'] for s in slots], schedule[index]['vector_ids'], 'slot coverage/order')
        expected = {'step_id': f'step-{index:04d}',
            'state': ('complete', 'complete', 'stopped', 'not_run')[index],
            'source_sha256': bindings['source_sha256'] if index < 2 else None,
            'application': 'not_applicable' if index < 2 else 'not_run', 'anchor_hits': None,
            'build_state': 'succeeded' if index < 2 else 'not_run', 'restored': True,
            'preflight': None, 'failure': failure if index == 2 else None, 'slots': slots}
        same(step, expected, 'step completion/declined mutation differs')
        if index >= 2:
            for slot in slots:
                same(slot, {'vector_id': slot['vector_id'], 'state': 'not_run', 'outcome': None,
                    'diagnostic': None, 'selector_presence': None, 'receipt': None,
                    'reason': 'operator-refused', 'evidence_sha256': stop}, 'unstarted slot claims evidence')
    expected_final = copy.deepcopy(prefix)
    expected_final['schema'] = 'corpus-adequacy.execution-observation.v0'
    expected_final['closure']['prefix_sha256'] = digest(prefix_raw)
    same(final, expected_final, 'final differs from no-admission closed prefix')
    return prefix


class Retained:
    """Bounded complete inventory, with relative digest lookup only."""
    def __init__(self, store):
        self.store = store
        self.inventory = store.json('inventory.json')
        inv = self.inventory
        closed(inv, 'schema stores files total_file_bytes finalization')
        need(inv['schema'] == 'refund.c2-baseline-retention.v0' and inv['stores'] == ['run'], 'retention schema')
        need(type(inv['files']) is list, 'inventory files shape')
        self.blobs = {}
        names, total = set(), 0
        for entry in inv['files']:
            closed(entry, 'store path size sha256')
            need(entry['store'] == 'run' and type(entry['path']) is str, 'inventory path')
            name = 'run/' + entry['path']
            need(name not in names, 'duplicate inventory path')
            names.add(name)
            raw = store.read(name)
            need(type(entry['size']) is int and entry['size'] == len(raw), 'inventory size')
            need(sha(raw) == hexdigest(entry['sha256']), 'inventory digest')
            total += len(raw)
            if '/blobs/' in name:
                need(name.rsplit('/', 1)[1] == sha(raw), 'blob address differs')
                self.blobs.setdefault(digest(raw), name)
        same(inv['total_file_bytes'], total, 'inventory total differs')
        need(set(store.names) == names | {'inventory.json'}, 'inventory incomplete/extra files')

    def blob(self, value):
        need(value in self.blobs, 'missing referenced blob')
        raw = self.store.read(self.blobs[value])
        need(digest(raw) == value, 'blob changed')
        return raw

    def operator_pair(self):
        """Repaired finalization format; no failed/incomplete pair is a success."""
        result_raw = self.store.read('run/operator/result.json')
        provenance = self.store.json('run/operator/provenance.json')
        need('run/operator/persistence-failure.json' not in self.store.names and
             'run/operator/result.pending.json' not in self.store.names, 'failed or pending persistence')
        need(provenance.get('schema') == 'refund.c2-baseline-provenance.v0', 'provenance schema')
        need(provenance.get('result_sha256') == sha(result_raw), 'provenance result binding')
        same(self.inventory['finalization'], {'state': 'complete', 'reason': None}, 'incomplete finalization')
        same(provenance.get('attempt'), {'state': 'closed', 'detail': None, 'source_bound': True,
            'judged_status': 'baseline-matches-oracle', 'persistence': 'provenance-written'},
            'failed attempt disposition')
        result = loads(result_raw)
        need(result.get('status') == 'baseline-matches-oracle', 'unsuccessful baseline result')
        return result, provenance



def receipt_set(retained, prefix, frozen):
    """Require a bijection between all baseline slots and all durable journals."""
    store = retained.store
    kinds = {kind: {} for kind in ('receipt', 'dispatch', 'call')}
    for name in store.names:
        match = re.search(r'/(receipt|dispatch|call)-([0-9]{6})(\.intent)?\.json$', name)
        if match:
            kind, index, intent = match.groups()
            need((intent == '.intent') == (kind == 'call'), 'journal filename')
            need(index not in kinds[kind], 'duplicate journal ordinal')
            kinds[kind][index] = name
    ids = sorted(frozen['oracle'])
    expected_ordinals = {f'{i:06d}' for i in range(len(ids))}
    for entries in kinds.values():
        need(set(entries) == expected_ordinals, 'missing/extra invocation journal')
    slots = prefix['steps'][1]['slots']
    same([slot['vector_id'] for slot in slots], ids, 'baseline coverage/order')
    used, reports, evidence_rows = set(), {}, {}
    for index, (case, slot) in enumerate(zip(ids, slots)):
        ordinal = f'{index:06d}'
        paths = [kinds[kind][ordinal] for kind in ('receipt', 'dispatch', 'call')]
        need({name.rsplit('/', 1)[0] for name in paths} == {'run/prefix/' + prefix['session']},
             'journal session directory differs')
        receipt_raw, dispatch_raw, intent_raw = (store.read(name) for name in paths)
        evidence = canonical(receipt_raw)
        ident = evidence['invocation_id']
        need(ident not in used, 'reused invocation identity')
        used.add(ident)
        same(canonical(intent_raw), {'dispatch_sha256': digest(dispatch_raw)}, 'dispatch intent differs')
        need(retained.blob(digest(receipt_raw)) == receipt_raw and
             retained.blob(digest(dispatch_raw)) == dispatch_raw, 'journal blob binding')
        reports[case] = invocation(slot, dispatch_raw, receipt_raw,
            retained.blob(evidence['stdout_sha256']), retained.blob(evidence['stderr_sha256']),
            frozen['packets'][case], frozen['oracle'][case], prefix['bindings']['source_sha256'],
            prefix['bindings']['backend_sha256'], prefix['session'], index)
        evidence_rows[case] = evidence
    return reports, evidence_rows


def success_result(actual, reports, receipts):
    cases = []
    for case, report_value in sorted(reports.items()):
        evidence = receipts[case]
        cases.append({'case_id': case, 'status': 'matches-oracle',
            'operations': [[row[k] for k in ('operation_id', 'result', 'reason')]
                           for row in report_value['operations']],
            'differences': [], 'detail': None,
            'invocation': {k: evidence[k] for k in ('returncode', 'abnormal', 'stdout_size', 'stderr_size')}})
    same(actual, {'schema': 'refund.c2-baseline-result.v0', 'status': 'baseline-matches-oracle',
        'stop': {'reason': 'operator-refused', 'shape': 'healthy-stop'},
        'cases': cases, 'non_claims': NON_CLAIMS}, 'full deterministic result differs')


def carrier_sources(carrier, retained):
    raw = carrier.read('source-map.json')
    need(sha(raw) == CARRIER_MAP_SHA256, 'carrier source-map pin differs')
    mapping = loads(raw)
    closed(mapping, 'schema commit files')
    need(mapping['schema'] == 'refund.c2-carrier-source-map.v0' and
         mapping['commit'] == CARRIER_COMMIT, 'carrier source revision differs')
    expected = {'source-map.json'}
    sources = {}
    for row in mapping['files']:
        closed(row, 'source_path export_path sha256 size git_mode')
        name = row['export_path']
        need(name not in expected and row['source_path'] == 'refund-reader-c2-2026-10/' + name,
             'carrier source path differs')
        expected.add(name)
        value = carrier.read(name)
        need(sha(value) == row['sha256'] and len(value) == row['size'], 'carrier source bytes differ')
        sources[name] = value
    need(set(carrier.names) == expected, 'carrier source set differs')
    pins = loads(sources['ca-source-pin.json'])
    instrument = pins['instrument']
    same(instrument['commit'], '7f4c8785fedbe43cfceb1d3e8cb26c7028215d08', 'instrument source commit differs')
    same(instrument['output_cap_bytes'], 4194304, 'instrument output cap differs')
    framed = hashlib.sha256(b'corpus-adequacy.tool-source.v0\n')
    for name, expected_hash in sorted(instrument['files'].items()):
        value = retained.store.read('run/export/' + name)
        need(sha(value) == expected_hash, 'instrument export source differs')
        name_bytes = name.encode()
        framed.update(str(len(name_bytes)).encode() + b'\n' + name_bytes)
        framed.update(str(len(value)).encode() + b'\n' + value)
    return sources, pins, 'sha256:' + framed.hexdigest()


def context_bindings(retained, sources, pins, tool_digest, provenance, prefix, frozen):
    closed(provenance, 'schema run_id argv carrier_python child_python pins subject_pin attempt result_sha256 '
                       'session prefix_sha256 final_sha256 prefix_path final_path dispatch_journal budget')
    same(provenance['pins'], pins, 'provenance source pins differ')
    same(provenance['subject_pin'], pins['subject']['files'], 'provenance subject pins differ')
    same(provenance['dispatch_journal'], 20, 'dispatch count differs')
    need(re.fullmatch(r'[0-9a-f]{32}', provenance['run_id']) is not None, 'run identity syntax')
    need(re.fullmatch(r'[0-9a-f-]{36}', provenance['session']) is not None, 'session identity syntax')
    need(provenance['session'] == prefix['session'], 'provenance session differs')
    child = provenance['child_python']
    closed(child, 'path version identity_claim')
    need(child['identity_claim'] == 'path-and-version-observation-only', 'interpreter claim differs')
    for value in (child['path'], child['version'], provenance['carrier_python']):
        need(type(value) is str and 0 < len(value) <= 4096, 'interpreter metadata shape')
    need(re.fullmatch(r'3\.[0-9]+\.[0-9]+', child['version']) is not None, 'child version metadata')
    need(int(child['version'].split('.')[1]) >= 11, 'child version below route minimum')
    context_raw = retained.store.read('run/operator/context.bin')
    context = loads(context_raw)
    same(context, {'schema': 'refund.c2-baseline-context.v0', 'run_id': provenance['run_id'],
        'pins': pins['instrument'], 'subject_pin': pins['subject']['files'], 'child_python': child,
        'carrier_sha256': sha(sources['ca_baseline.py']), 'consumer_sha256': sha(sources['consumer.py']),
        'budget_sha256': sha(sources['budget.py'])}, 'operator context differs from source/provenance')
    subject, schedule = subject_inputs(retained.store, frozen)
    same(pins['oracle']['expectations.json'], FROZEN_HASHES['expectations.json'], 'oracle pin differs')
    same(pins['oracle']['fixtures.sha256.json'], FROZEN_HASHES['fixtures.sha256.json'], 'fixture manifest pin differs')
    for name, expected_hash in pins['subject']['files'].items():
        need(sha(retained.store.read('run/subject/' + name)) == expected_hash, 'subject pin differs')
    native_bindings = prefix['bindings']
    environment = native_bindings['environment_sha256']
    need(type(environment) is str and environment.startswith('sha256:'), 'environment digest syntax')
    hexdigest(environment[7:])
    expected = dict(subject, tool_version='0.8.0', tool_commit=None, tool_source_state='unresolved',
        tool_content_sha256=tool_digest,
        backend_sha256=digest(native({'profile': 'trusted-local', 'tool_content_sha256': tool_digest})),
        environment_sha256=environment, context_sha256=digest(context_raw),
        interpreter_identity=digest(sources['ca_baseline.py']), policy_identity=digest(POLICY))
    same(native_bindings, expected, 'native observation bindings differ')
    return expected, schedule, context_raw


def complete_graph(retained, prefix_name, final_name, prefix, context_raw, pins, receipts):
    """Account for every retained file and both copies of every referenced blob."""
    store = retained.store
    before, after = prefix_name.rsplit('/', 1)[0], final_name.rsplit('/', 1)[0]
    names = {prefix_name, final_name, 'run/operator/context.bin', 'run/operator/result.json',
             'run/operator/provenance.json', before + '/intent.json'}
    names.update('run/export/' + name for name in pins['instrument']['files'])
    names.update('run/subject/' + name for name in pins['subject']['files'])
    same(canonical(store.read(before + '/intent.json')),
        {'schema': 'corpus-adequacy.observation-intent.v0', 'session': prefix['session'],
         'bindings': prefix['bindings'], 'schedule': prefix['schedule']}, 'prefix intent differs')
    for index in range(2):
        name = before + f'/step-{index:04d}.json'
        names.add(name)
        same(canonical(store.read(name)), prefix['steps'][index], 'step checkpoint differs')
    blobs = {digest(context_raw), prefix['cleanup']['evidence_sha256'],
             prefix['steps'][2]['failure']['evidence_sha256']}
    for index, evidence in enumerate(receipts.values()):
        names.update(before + '/' + name for name in
            (f'receipt-{index:06d}.json', f'dispatch-{index:06d}.json', f'call-{index:06d}.intent.json'))
        blobs.add(digest(native(evidence)))
        blobs.update(evidence[key] for key in ('dispatch_sha256', 'stdout_sha256', 'stderr_sha256'))
    for value in blobs:
        raw = retained.blob(value)
        for root in (before, after):
            name = root + '/blobs/' + value[7:]
            names.add(name)
            need(store.read(name) == raw, 'prefix/close blob copy differs')
    prefix_blob = after + '/blobs/' + sha(store.read(prefix_name))
    names.add(prefix_blob)
    need(store.read(prefix_blob) == store.read(prefix_name), 'close prefix blob differs')
    need(set(store.names) == names | {'inventory.json'}, 'incomplete/extra baseline graph')


def budget_snapshot(retained, provenance, prefix_name, receipts):
    """Reconcile the recorded early snapshot; never infer unretained late samples."""
    store, budget = retained.store, provenance['budget']
    closed(budget, 'limit reserve min_free max_calls calls_reserved verified_receipts decisions free_samples')
    constants = {'limit': 268435456, 'reserve': 134217728, 'min_free': 5368709120,
                 'max_calls': 20, 'calls_reserved': 20, 'verified_receipts': 20}
    same({key: budget[key] for key in constants}, constants, 'budget policy/counters differ')
    limit, reserve = constants['limit'], constants['reserve']
    decisions, targets = [], []
    used = 0
    def write(name, size):
        decisions.append({'label': 'operator-write', 'path': name.rsplit('/', 1)[1],
                          'bytes': size, 'used': used, 'limit': limit, 'ok': True})
        targets.append(name.rsplit('/', 1)[0][4:])
    def interval(label, count, allowance=0):
        decisions.append({'label': label, 'used': count, 'reserve': reserve,
                          'copy_allowance': allowance, 'limit': limit, 'ok': True})
        targets.append(None)
    for group in ('run/export/', 'run/subject/'):
        for name in sorted(n for n in store.names if n.startswith(group)):
            size = len(store.read(name))
            write(name, size)
            used += size
    context_raw = store.read('run/operator/context.bin')
    write('run/operator/context.bin', len(context_raw))
    used += len(context_raw)
    interval('prefix-entry', used)
    prefix_root = prefix_name.rsplit('/', 1)[0] + '/'
    used += len(context_raw) + len(store.read(prefix_root + 'intent.json')) + len(store.read(prefix_root + 'step-0000.json'))
    seen = {digest(context_raw)}
    for index, evidence in enumerate(receipts.values()):
        for name in (f'call-{index:06d}.intent.json', f'dispatch-{index:06d}.json', f'receipt-{index:06d}.json'):
            used += len(store.read(prefix_root + name))
        references = {digest(native(evidence))} | {evidence[k] for k in ('dispatch_sha256', 'stdout_sha256', 'stderr_sha256')}
        for value in references - seen:
            used += len(retained.blob(value))
        seen.update(references)
        interval('prefix-terminal' if index == 19 else 'prefix-interhook', used)
    prefix_size = sum(len(store.read(name)) for name in store.names if name.startswith(prefix_root))
    initial_size = sum(len(store.read(name)) for name in store.names
                       if name.startswith(('run/export/', 'run/subject/')))
    interval('close-entry', initial_size + len(context_raw) + prefix_size, prefix_size)
    result_raw = store.read('run/operator/result.json')
    provenance_raw = store.read('run/operator/provenance.json')
    final_run_bytes = retained.inventory['total_file_bytes']
    used = final_run_bytes - len(result_raw) - len(provenance_raw)
    # The draft was serialized before final-artifacts and result.pending write.
    sample_count = sum(3 if target is not None else 2 for target in targets)
    draft = copy.deepcopy(provenance)
    draft['attempt']['persistence'] = 'pending'
    draft['budget']['decisions'] = decisions
    draft['budget']['free_samples'] = budget['free_samples'][:sample_count]
    draft_bytes = (json.dumps(draft, sort_keys=True, separators=(',', ':'), ensure_ascii=True) + '\n').encode()
    decisions.append({'label': 'final-artifacts', 'used': used, 'bytes': 2 * len(result_raw) + len(draft_bytes) + 65536,
                      'limit': limit, 'ok': True})
    targets.append(None)
    write('run/operator/result.pending.json', len(result_raw))
    same(budget['decisions'], decisions, 'budget snapshot decision arithmetic differs')
    for row in decisions:
        requested = row.get('bytes', row.get('reserve', 0) + row.get('copy_allowance', 0))
        need(row['used'] + requested <= limit, 'budget reservation exceeds limit')
    argv = provenance['argv']
    need(type(argv) is list and len(argv) == 5 and argv[:2] == ['ca_baseline.py', '--ca-repo']
         and argv[3] == '--run-root' and all(type(v) is str and 0 < len(v) <= 4096 for v in argv), 'historical argv shape')
    history_root = argv[4]
    samples = budget['free_samples']
    need(type(samples) is list and len(samples) == sum(3 if t is not None else 2 for t in targets), 'free sample coverage differs')
    temp_label = samples[0]['path']
    need(type(temp_label) is str and 0 < len(temp_label) <= 4096, 'historical temp label')
    offset = 0
    for row, target in zip(decisions, targets):
        roles = [('temp', temp_label), ('store', history_root)]
        if target is not None:
            roles.append(('target', history_root + '/' + target))
        for role, path in roles:
            sample = samples[offset]
            offset += 1
            closed(sample, 'free label ok path probed role')
            need(type(sample['free']) is int and sample['free'] >= constants['min_free'], 'historical free sample below floor')
            same(sample, {'free': sample['free'], 'label': row['label'], 'ok': True,
                          'path': path, 'probed': path, 'role': role}, 'historical free sample identity differs')
    retained_bytes = final_run_bytes + len(store.read('inventory.json'))
    need(2 * final_run_bytes + len(store.read('inventory.json')) <= limit, 'terminal logical two-store total exceeds limit')
    return {'snapshot_decisions': len(decisions), 'snapshot_free_samples': len(samples),
            'snapshot_max_used': max(row['used'] for row in decisions),
            'logical_run_bytes': final_run_bytes, 'retained_bytes': retained_bytes,
            'run_plus_retained_bytes': final_run_bytes + retained_bytes,
            'late_free_samples_retained': False, 'peak_measured': False}


def inspect_package(retained, inputs, carrier):
    """Semantic graph inspection; only verify() adds fixed artifact acceptance."""
    store = retained.store
    frozen = frozen_inputs(inputs)
    sources, pins, tool_digest = carrier_sources(carrier, retained)
    result, provenance = retained.operator_pair()
    def only(pattern):
        names = [name for name in store.names if re.fullmatch(pattern, name)]
        need(len(names) == 1, 'missing/ambiguous observation')
        return names[0]
    prefix_name = only(r'run/prefix/[0-9a-f-]{36}/prefix\.json')
    final_name = only(r'run/close/[0-9a-f-]{36}/final\.json')
    prefix_raw, final_raw = store.read(prefix_name), store.read(final_name)
    prefix = canonical(prefix_raw)
    bindings, schedule, context_raw = context_bindings(retained, sources, pins, tool_digest, provenance, prefix, frozen)
    need(provenance['prefix_sha256'] == sha(prefix_raw) and provenance['final_sha256'] == sha(final_raw),
         'provenance prefix/final binding differs')
    observation(prefix_raw, final_raw, bindings, schedule, retained.blob)
    reports, receipts = receipt_set(retained, prefix, frozen)
    need(len(reports) == len(receipts) == 20 and sum(len(r['operations']) for r in reports.values()) == 25,
         'baseline coverage differs')
    success_result(result, reports, receipts)
    complete_graph(retained, prefix_name, final_name, prefix, context_raw, pins, receipts)
    budget = budget_snapshot(retained, provenance, prefix_name, receipts)
    # Historical labels are compared lexically; never resolved or opened.
    history_root = provenance['argv'][4]
    need(provenance['prefix_path'] == history_root + '/' + prefix_name[4:] and
         provenance['final_path'] == history_root + '/' + final_name[4:], 'historical observation labels differ')
    return {'schema': 'refund.offline-baseline-verification.v0', 'cases': len(reports),
        'operation_rows': sum(len(r['operations']) for r in reports.values()),
        'receipts': len(receipts), 'dispatches': len(receipts),
        'inventory_files': len(retained.inventory['files']), 'result_sha256': sha(store.read('run/operator/result.json')),
        'instrument': {'native_commit': bindings['tool_commit'], 'native_source_state': bindings['tool_source_state'],
                       'external_source_commit': pins['instrument']['commit'], 'exported_source_files': len(pins['instrument']['files'])},
        'budget': budget, 'execution': 'not-performed', 'custody_authenticated': False}


def verify(retained=None, c2_inputs=None, carrier_inputs=None):
    with Store(retained or HERE / 'record/baseline-c2') as store, \
            Store(c2_inputs or HERE / 'record/c2') as inputs, \
            Store(carrier_inputs or HERE / 'record/c2-carrier') as carrier:
        for name, expected in [('inventory.json', ACCEPTED_INVENTORY_SHA256),
                               ('run/operator/result.json', ACCEPTED_RESULT_SHA256),
                               ('run/operator/provenance.json', ACCEPTED_PROVENANCE_SHA256)]:
            need(sha(store.read(name)) == expected, 'not the pinned baseline artifact')
        return inspect_package(Retained(store), inputs, carrier)


def main():
    parser = argparse.ArgumentParser(description=__doc__.split('\n', 1)[0])
    parser.add_argument('--retained', type=Path)
    args = parser.parse_args()
    try:
        result = verify(args.retained)
    except (Refused, OSError, KeyError, TypeError, ValueError, IndexError) as exc:
        print('refused: ' + str(exc), file=sys.stderr)
        return 2
    print(json.dumps(result, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main())
