"""Selection-bound status projector (PROJECTION-CONTRACT.md; MAPPING_REVISION is its sha256).

Projects exactly one native C1/C2/C3 result, selected by profile, native subject and axis, onto
established | not_established | failed | unsupported, and keeps the native result and its context
beside the projection. Packet and oracle bytes are bound by sha256. Any binding, selection or shape
failure yields a no_claim record, never a status. Stdlib only; imports no reader, reproducer or
Corpus Adequacy code.

Usage (paths relative to the current directory):
  projector.py project   --profile P --packet F --packet-sha256 H --oracle F --oracle-sha256 H
                         (--dispatch ID | --dispatch-null | --operation ID [--report ID]) --axis A
  projector.py refusal   --profile P --packet F --packet-sha256 H --refusal-oracle F --oracle-sha256 H --key K
  projector.py diagnosis --diagnosis F
  projector.py wrap      --profile P --packet F --packet-sha256 H --operation ID --report ID --axis A
                         [--pin PATH=SHA256]... [--timeout S] -- ARGV...
Prints exactly one JSON record and exits 0 whenever a record exists; exits 64 on a usage error.
Exit 2 is never used: it is the native readers' refusal code.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

MAPPING_REVISION = '07148c94abff8882668e9f327c2cddc5d6280e7ae4cf0d0aed80f0426f22b7de'
C1, C2, C3 = 'refund.synthetic-c1.v0', 'refund.synthetic-c2.v0', 'refund.synthetic-c3.v0'
READER = {C1: 'C1', C2: 'C2', C3: 'C3'}
AXES = {C1: ('c1', 'no_forbidden_dispatch'), C2: ('c2',), C3: ('definite_support', 'uncertainty_calibration')}
APPLICABILITY_AXES = frozenset(('c1', 'definite_support', 'uncertainty_calibration'))
RESULTS = ('ESTABLISHED', 'CONTRADICTED', 'NOT_ESTABLISHED')
REFUSAL_CATEGORIES = ('io', 'parse', 'schema', 'binding')
SUBJECT_FIELDS = {C1: {'dispatch'}, C2: {'operation'}, C3: {'operation', 'report'}}
C3_REPORT_KEYS = frozenset(('schema', 'case_id', 'operation_id', 'packet_sha256', 'source_class',
                            'evidence_basis', 'delivery_coverage', 'claims', 'non_claims'))
C3_CLAIM_KEYS = frozenset(('report_id', 'report_seq', 'reported_outcome', 'eligible_delivery_ids',
                           'eligible_result_ids', 'conflict', 'definite_support', 'uncertainty_calibration'))
REFUSED_LINE = re.compile(rb'^refused: (io|parse|schema|binding): ')
HEX64 = re.compile(r'^[0-9a-f]{64}$')
EX_USAGE = 64
STDOUT_RETAIN = 65536


class NoClaim(Exception):
    def __init__(self, reason):
        super().__init__(reason)
        self.reason = reason


class ExecutionState(Exception):
    def __init__(self, state):
        super().__init__(state)
        self.state = state


class Usage(Exception):
    pass


class Parser(argparse.ArgumentParser):
    def error(self, message):
        raise Usage(message)

    def exit(self, status=0, message=None):
        raise Usage(message or 'exit requested')


def sha256(raw):
    return hashlib.sha256(raw).hexdigest()


def read_bound(*, path, declared, reason):
    try:
        raw = Path(path).read_bytes()
    except OSError:
        raise NoClaim('input_unreadable') from None
    if sha256(raw) != declared:
        raise NoClaim(reason)
    return raw


def strict_json(raw, reason):
    def pairs(items):
        keys = [k for k, _ in items]
        if len(set(keys)) != len(keys):
            raise ValueError('duplicate key')
        return dict(items)

    def constant(name):
        raise ValueError(name)

    try:
        return json.loads(raw.decode('utf-8'), object_pairs_hook=pairs, parse_constant=constant)
    except (UnicodeDecodeError, ValueError, RecursionError):
        raise NoClaim(reason) from None


# Seams. Each is keyword-only so a named faulty mapper can replace exactly one of them in witnesses.py.

def map_status(*, native, has_applicability):
    if native['result'] == 'ESTABLISHED' and (not has_applicability or native['applicable'] is True):
        return 'established'
    return 'not_established'


def native_for(*, profile, row, axis):
    if profile == C1:
        if axis == 'c1':
            return {'applicable': row.get('c1_applicable'), 'result': row.get('c1_result'),
                    'reason': row.get('c1_reason')}
        return {'result': row.get('no_forbidden_dispatch'), 'reason': row.get('no_forbidden_dispatch_reason')}
    if profile == C2:
        return {'result': row.get('result'), 'reason': row.get('reason')}
    value = row.get(axis)
    return dict(value) if isinstance(value, dict) else value


def c1_subject(*, packet, dispatch_id):
    return {'case_id': packet['case_id'], 'dispatch_id': dispatch_id}


def c2_context(*, packet, case):
    return {'effect_coverage': packet['effect_coverage'], 'source_class': packet['source_class'],
            'epoch': case['epoch'], 'closure_contradicted': case['closure_contradicted'],
            'unattributed_blocking_effect_ids': case['unattributed_blocking_effect_ids']}


def c2_diagnostics(*, op_row, case):
    """Retained ID sets: committed always; late or key-hint sets when packet-wide context invokes them."""
    out = {'committed_effect_ids': op_row['committed_effect_ids']}
    if case['closure_contradicted']:
        out['late_effect_ids'] = op_row['late_effect_ids']
    if case['unattributed_blocking_effect_ids']:
        out['key_hint_effect_ids'] = op_row['key_hint_effect_ids']
    return out


def select_report(*, rows, report_id, axis, events):
    matches = [r for r in rows if isinstance(r, dict) and r.get('report_id') == report_id]
    seen = [e for e in events if isinstance(e, dict) and e.get('kind') == 'client_report'
            and e.get('id') == report_id]
    if len(matches) != 1 or len(seen) != 1:
        raise NoClaim('subject_not_found')
    if seen[0].get('seq') != matches[0].get('report_seq'):
        raise NoClaim('subject_binding_mismatch')
    return matches[0]


def claim_record(*, profile, row_id, subject, axis, native, context, binding, has_applicability):
    return {'kind': 'claim', 'profile': profile, 'row': row_id, 'subject': subject, 'axis': axis,
            'native': native, 'context': context,
            'projected': map_status(native=native, has_applicability=has_applicability), 'binding': binding}


def refusal_record(*, profile, native, binding):
    return {'kind': 'refusal_record', 'profile': profile,
            'subject': {'packet_only': True, 'packet_sha256': binding['packet_sha256']},
            'native': native, 'projected': 'failed', 'semantic_rows': 0, 'binding': binding}


def bind_report(*, report, packet, packet_sha256):
    if report['packet_sha256'] != packet_sha256:
        raise ExecutionState('output_binding_mismatch')
    if report['case_id'] != packet.get('case_id') or report['operation_id'] != packet.get('operation_id'):
        raise ExecutionState('output_binding_mismatch')


def verify_pins(*, pins):
    """Pre-spawn precondition only: every pinned file exists with the pinned bytes. Not reader identity."""
    for path, digest in pins:
        target = Path(path)
        if not target.is_file():
            return 'not_started:reader_missing'
        if sha256(target.read_bytes()) != digest:
            return 'not_started:pin_mismatch'
    return None


def _retained(raw, prefix):
    raw = raw or b''
    kept = raw[:STDOUT_RETAIN]
    return {f'{prefix}_sha256': sha256(raw), f'{prefix}_bytes': len(raw), f'{prefix}_retained_bytes': len(kept),
            f'{prefix}_truncated': len(kept) < len(raw), f'{prefix}_base64': base64.b64encode(kept).decode()}


def execution_record(*, state, completed=None, stdout=None, stderr=None, timeout_seconds=None):
    """No-claim record of a wrapper outcome. Both streams are retained up to STDOUT_RETAIN bytes each, with
    the full-stream digest and explicit truncation metadata. A timeout has no child exit code."""
    if completed is not None:
        stdout, stderr = completed.stdout, completed.stderr
    record = {'kind': 'execution_record', 'state': state, 'projected_claims': 0,
              'returncode': None if completed is None else completed.returncode}
    if timeout_seconds is not None:
        record['timeout_seconds'] = timeout_seconds
        record['child_exit'] = 'killed after timeout; no exit code recorded'
    if stdout is not None or stderr is not None:
        record.update(_retained(stdout, 'stdout'), **_retained(stderr, 'stderr'))
    return record


def classify_process(*, profile, completed):
    """Record for any outcome other than a successful exit; None means: read stdout as a report.

    A wrapped exit 2 is never a refusal record. Neither a pin nor a file named in argv establishes which
    code ran (`python3 -c CODE pinned_file` leaves the pin unused), and the interpreter itself exits 2
    when it cannot open a script. A refusal-shaped first stderr line is kept as an observation only.
    Refusal records come from the frozen refusal oracles through the `refusal` subcommand."""
    code = completed.returncode
    if code < 0:
        return execution_record(state=f'terminated_by_signal:{-code}', completed=completed)
    if code == 2 and completed.stdout == b'':
        record = execution_record(state='unclassified_exit:2', completed=completed)
        shape = REFUSED_LINE.match(completed.stderr)
        record['stderr_refusal_shape'] = shape.group(1).decode() if shape else None
        return record
    if code != 0:
        return execution_record(state=f'unexpected_exit:{code}', completed=completed)
    return None


# Validation and selection.

def _check_native(native, has_applicability):
    if not isinstance(native, dict):
        raise NoClaim('native_malformed')
    if has_applicability:
        ok = (set(native) == {'applicable', 'result', 'reason'} and isinstance(native['applicable'], bool)
              and native['result'] in RESULTS + (None,) and (native['result'] is None) == (not native['applicable']))
    else:
        ok = set(native) == {'result', 'reason'} and native['result'] in RESULTS
    if not ok or not isinstance(native.get('reason'), str) or not native['reason']:
        raise NoClaim('native_malformed')


def _check_selection(profile, args):
    given = {f for f in ('dispatch', 'operation', 'report') if getattr(args, f, None) is not None}
    if getattr(args, 'dispatch_null', False):
        given.add('dispatch')
    if given - SUBJECT_FIELDS[profile]:
        raise NoClaim('subject_field_not_in_profile')
    if SUBJECT_FIELDS[profile] - given:
        raise NoClaim('selection_missing')
    if args.axis not in AXES[profile]:
        raise NoClaim('axis_not_in_profile')


def _packet(args):
    raw = read_bound(path=args.packet, declared=args.packet_sha256, reason='packet_binding_mismatch')
    packet = strict_json(raw, 'packet_unparseable')
    if not isinstance(packet, dict) or packet.get('schema') != args.profile:
        raise NoClaim('profile_mismatch')
    if not isinstance(packet.get('case_id'), str):
        raise NoClaim('packet_field_missing')
    return raw, packet


def _require(mapping, keys, reason):
    if not isinstance(mapping, dict) or any(k not in mapping for k in keys):
        raise NoClaim(reason)


def _select_c1(packet, case, args, binding):
    dispatch_id = None if args.dispatch_null else args.dispatch
    _require(packet, ('target_dispatch', 'coverage', 'source_class'), 'packet_field_missing')
    if packet['target_dispatch'] != dispatch_id:
        raise NoClaim('subject_not_found')
    _require(case, ('c1_applicable', 'c1_result', 'c1_reason', 'no_forbidden_dispatch',
                    'no_forbidden_dispatch_reason'), 'oracle_row_malformed')
    has_applicability = args.axis in APPLICABILITY_AXES
    native = native_for(profile=C1, row=case, axis=args.axis)
    _check_native(native, has_applicability)
    return claim_record(profile=C1, row_id=f"C1/{packet['case_id']}/{args.axis}",
                        subject=c1_subject(packet=packet, dispatch_id=dispatch_id), axis=args.axis,
                        native=native, context={'coverage': packet['coverage'],
                                                'source_class': packet['source_class']},
                        binding=binding, has_applicability=has_applicability)


def _select_c2(packet, case, args, binding):
    _require(packet, ('admissions', 'effect_coverage', 'source_class'), 'packet_field_missing')
    _require(case, ('epoch', 'closure_contradicted', 'unattributed_blocking_effect_ids', 'operations'),
             'oracle_row_malformed')
    rows = [o for o in case['operations'] if isinstance(o, dict) and o.get('operation_id') == args.operation]
    admitted = [a for a in packet['admissions'] if isinstance(a, dict) and a.get('operation_id') == args.operation]
    if len(rows) != 1 or not admitted:
        raise NoClaim('subject_not_found')
    op_row = rows[0]
    _require(op_row, ('result', 'reason', 'committed_effect_ids', 'late_effect_ids', 'key_hint_effect_ids'),
             'oracle_row_malformed')
    native = native_for(profile=C2, row=op_row, axis=args.axis)
    _check_native(native, False)
    native = {**native, **c2_diagnostics(op_row=op_row, case=case)}
    return claim_record(profile=C2, row_id=f"C2/{packet['case_id']}/{args.operation}/{args.axis}",
                        subject={'case_id': packet['case_id'], 'operation_id': args.operation}, axis=args.axis,
                        native=native, context=c2_context(packet=packet, case=case), binding=binding,
                        has_applicability=False)


def _select_c3(packet, rows, args, binding):
    _require(packet, ('operation_id', 'events', 'delivery_coverage', 'source_class'), 'packet_field_missing')
    if packet['operation_id'] != args.operation:
        raise NoClaim('subject_not_found')
    if not isinstance(rows, list) or not isinstance(packet['events'], list):
        raise NoClaim('oracle_row_malformed')
    row = select_report(rows=rows, report_id=args.report, axis=args.axis, events=packet['events'])
    native = native_for(profile=C3, row=row, axis=args.axis)
    _check_native(native, True)
    return claim_record(profile=C3, row_id=f"C3/{packet['case_id']}/{row['report_id']}/{args.axis}",
                        subject={'case_id': packet['case_id'], 'operation_id': args.operation,
                                 'report_id': row['report_id'], 'report_seq': row['report_seq']},
                        axis=args.axis, native=native,
                        context={'delivery_coverage': packet['delivery_coverage'],
                                 'source_class': packet['source_class'], 'conflict': row.get('conflict')},
                        binding=binding, has_applicability=True)


def cmd_project(args, command):
    _check_selection(args.profile, args)
    packet_raw, packet = _packet(args)
    oracle_raw = read_bound(path=args.oracle, declared=args.oracle_sha256, reason='oracle_binding_mismatch')
    oracle = strict_json(oracle_raw, 'oracle_unparseable')
    if not isinstance(oracle, dict) or packet['case_id'] not in oracle:
        raise NoClaim('oracle_row_missing')
    binding = {'packet_sha256': sha256(packet_raw), 'oracle_sha256': sha256(oracle_raw),
               'mapping_revision': MAPPING_REVISION}
    select = {C1: _select_c1, C2: _select_c2, C3: _select_c3}[args.profile]
    return select(packet, oracle[packet['case_id']], args, binding)


def cmd_refusal(args, command):
    packet_raw = read_bound(path=args.packet, declared=args.packet_sha256, reason='packet_binding_mismatch')
    oracle_raw = read_bound(path=args.refusal_oracle, declared=args.oracle_sha256, reason='oracle_binding_mismatch')
    oracle = strict_json(oracle_raw, 'oracle_unparseable')
    if Path(args.packet).name not in (args.key, args.key + '.json'):
        raise NoClaim('refusal_key_packet_mismatch')
    if not isinstance(oracle, dict) or args.key not in oracle:
        raise NoClaim('refusal_key_missing')
    entry = oracle[args.key]
    if args.profile == C1:
        static = (isinstance(entry, dict) and set(entry) == {'exit', 'stdout'} and type(entry['exit']) is int
                  and entry['exit'] == 2 and entry['stdout'] == '')
        if not static:
            raise NoClaim('refusal_entry_unsupported')
        native = {'exit': 2, 'stdout': '', 'category': None}
    elif isinstance(entry, str) and entry in REFUSAL_CATEGORIES:
        native = {'exit': 2, 'stdout': '', 'category': entry}
    else:
        raise NoClaim('refusal_entry_unsupported')
    return refusal_record(profile=args.profile, native=native,
                          binding={'packet_sha256': sha256(packet_raw), 'oracle_sha256': sha256(oracle_raw)})


def cmd_diagnosis(args, command):
    try:
        raw = Path(args.diagnosis).read_bytes()
    except OSError:
        raise NoClaim('input_unreadable') from None
    doc = strict_json(raw, 'diagnosis_unparseable')
    if not isinstance(doc, dict) or set(doc) != {'subject', 'profile', 'packet', 'diagnosis'}:
        raise NoClaim('diagnosis_malformed')
    if doc['packet'] is not None:
        raise NoClaim('diagnosis_attached_to_packet')
    diagnosis = doc['diagnosis']
    if (not isinstance(diagnosis, dict) or diagnosis.get('status') != 'unsupported'
            or diagnosis.get('basis') != 'synthetic-wrapper-control' or doc['profile'] not in AXES):
        raise NoClaim('diagnosis_not_synthetic')
    return {'kind': 'unsupported', 'subject': doc['subject'], 'profile': doc['profile'],
            'basis': diagnosis['basis'], 'projected': 'unsupported', 'semantic_rows': 0,
            'attached_to_corpus_input': False, 'binding': {'diagnosis_sha256': sha256(raw)}}


def _valid_report(report):
    return (isinstance(report, dict) and set(report) == C3_REPORT_KEYS and report['schema'] == 'refund.c3-report.v0'
            and isinstance(report['claims'], list)
            and all(isinstance(c, dict) and set(c) == C3_CLAIM_KEYS for c in report['claims']))


def _pin(text):
    path, sep, digest = text.rpartition('=')
    if not sep or not path or not HEX64.match(digest):
        raise Usage(f'bad --pin {text!r}')
    return path, digest


def cmd_wrap(args, command):
    if args.profile != C3:
        raise NoClaim('wrap_profile_unsupported')
    _check_selection(args.profile, args)
    packet_raw, packet = _packet(args)
    if not command:
        raise Usage('wrap needs a command after --')
    pins = [_pin(p) for p in args.pin]
    binding = {'packet_sha256': sha256(packet_raw)}
    state = verify_pins(pins=pins)
    if state:
        return execution_record(state=state)
    try:
        completed = subprocess.run(command, capture_output=True, timeout=args.timeout, check=False)
    except subprocess.TimeoutExpired as expired:
        return execution_record(state='timed_out', stdout=expired.stdout, stderr=expired.stderr,
                                timeout_seconds=args.timeout)
    except OSError:
        return execution_record(state='not_started:spawn_failed')
    early = classify_process(profile=args.profile, completed=completed)
    if early is not None:
        return early
    try:
        report = strict_json(completed.stdout, 'output_unparseable')
    except NoClaim:
        return execution_record(state='output_unparseable', completed=completed)
    if not _valid_report(report):
        return execution_record(state='output_schema_invalid', completed=completed)
    try:
        bind_report(report=report, packet=packet, packet_sha256=binding['packet_sha256'])
    except ExecutionState as failed:
        return execution_record(state=failed.state, completed=completed)
    binding.update(report_sha256=sha256(completed.stdout), mapping_revision=MAPPING_REVISION)
    return _select_c3(packet, report['claims'], args, binding)


def _parser():
    parser = Parser(prog='projector.py')
    sub = parser.add_subparsers(dest='cmd', required=True, parser_class=Parser)
    profiles = sorted(AXES)
    axes = sorted({a for v in AXES.values() for a in v})

    def bound_packet(p):
        p.add_argument('--profile', required=True, choices=profiles)
        p.add_argument('--packet', required=True)
        p.add_argument('--packet-sha256', required=True)

    def selection(p):
        group = p.add_mutually_exclusive_group()
        group.add_argument('--dispatch')
        group.add_argument('--dispatch-null', action='store_true')
        p.add_argument('--operation')
        p.add_argument('--report')
        p.add_argument('--axis', required=True, choices=axes)

    project = sub.add_parser('project')
    bound_packet(project)
    project.add_argument('--oracle', required=True)
    project.add_argument('--oracle-sha256', required=True)
    selection(project)
    refusal = sub.add_parser('refusal')
    bound_packet(refusal)
    refusal.add_argument('--refusal-oracle', required=True)
    refusal.add_argument('--oracle-sha256', required=True)
    refusal.add_argument('--key', required=True)
    sub.add_parser('diagnosis').add_argument('--diagnosis', required=True)
    wrap = sub.add_parser('wrap')
    bound_packet(wrap)
    selection(wrap)
    wrap.add_argument('--pin', action='append', default=[])
    wrap.add_argument('--timeout', type=float, default=20.0)
    return parser


HANDLERS = {'project': cmd_project, 'refusal': cmd_refusal, 'diagnosis': cmd_diagnosis, 'wrap': cmd_wrap}


def run(argv):
    """Return (exit_code, record). exit 64 and record None on a usage error."""
    argv = list(argv)
    command = []
    if argv[:1] == ['wrap'] and '--' in argv:
        split = argv.index('--')
        argv, command = argv[:split], argv[split+1:]
    try:
        args = _parser().parse_args(argv)
        return 0, HANDLERS[args.cmd](args, command)
    except Usage:
        return EX_USAGE, None
    except NoClaim as refused:
        return 0, {'kind': 'no_claim', 'reason': refused.reason, 'projected_claims': 0}


def main():
    code, record = run(sys.argv[1:])
    if record is None:
        print('usage error; see the module docstring', file=sys.stderr)
    else:
        sys.stdout.write(json.dumps(record, sort_keys=True)+'\n')
    return code


if __name__ == '__main__':
    sys.exit(main())
