#!/usr/bin/env python3
"""Offline C3 interpretation of fixture-declared deliveries; no effect execution."""
import hashlib
import json
import math
from pathlib import Path
import re
import sys

MAX_BYTES = 1048576
NON_CLAIMS = ['no-live-execution', 'no-runtime-coverage-proof',
              'no-provider-authentication', 'no-effect-truth', 'no-C2', 'no-adequacy-score']


class Refusal(ValueError):
    def __init__(self, category, message):
        super().__init__(message)
        self.category = category


def require(condition, category, message):
    if not condition:
        raise Refusal(category, message)


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'parse', 'duplicate decoded key')
        result[key] = value
    return result


def integer(token):
    # C1 supplemental bound, copied with attribution; no mutable C1 import.
    digits = token.lstrip('-')
    require(len(digits) <= 4300, 'parse', 'integer token too long')
    value = 0
    for start in range(0, len(digits), 100):
        chunk = digits[start:start+100]
        value = value * 10**len(chunk) + int(chunk)
    return -value if token.startswith('-') else value


def finite(token):
    value = float(token)
    require(math.isfinite(value), 'parse', 'nonfinite number')
    return value


def nonfinite(token):
    raise Refusal('parse', 'nonfinite constant')


def parse(raw):
    require(len(raw) <= MAX_BYTES, 'parse', 'input too large')
    try:
        packet = json.loads(raw.decode('utf-8'), object_pairs_hook=unique_object,
                            parse_int=integer, parse_float=finite, parse_constant=nonfinite)
    except (UnicodeError, json.JSONDecodeError, RecursionError) as exc:
        raise Refusal('parse', 'invalid UTF8 or JSON') from exc
    pending = [(packet, 0)]
    while pending:
        value, parent = pending.pop()
        if type(value) in (dict, list):
            depth = parent+1
            require(depth <= 32, 'parse', 'container depth exceeded')
            pending.extend((child, depth) for child in
                           (value.values() if type(value) is dict else value))
    return packet


def identifier(value):
    return type(value) is str and re.fullmatch(r'[A-Za-z0-9_.:-]{1,128}', value) is not None


def closed(value, fields):
    require(type(value) is dict and set(value) == fields, 'schema', 'closed object required')


def validate(packet):
    closed(packet, {'schema', 'case_id', 'source_class', 'operation_id', 'delivery_coverage', 'events'})
    require(packet['schema'] == 'refund.synthetic-c3.v0', 'schema', 'unknown schema')
    require(packet['source_class'] == 'synthetic_fixture', 'schema', 'unknown source class')
    require(packet['delivery_coverage'] in ('complete', 'incomplete'), 'schema', 'unknown coverage')
    require(identifier(packet['case_id']) and identifier(packet['operation_id']), 'schema', 'invalid root ID')
    events = packet['events']
    require(type(events) is list and 1 <= len(events) <= 1000, 'schema', 'event count')
    shapes = {'dispatch': set(), 'provider_result': {'status'}, 'delivery': {'result_ref'},
              'client_report': {'outcome'}}
    common = {'id', 'seq', 'kind', 'operation_id'}
    # Complete schema stage before any cross-record binding check.
    for e in events:
        require(type(e) is dict and type(e.get('kind')) is str and e['kind'] in shapes,
                'schema', 'unknown event kind')
        closed(e, common | shapes[e['kind']])
        require(identifier(e['id']) and identifier(e['operation_id']), 'schema', 'invalid event ID')
        require(type(e['seq']) is int and 1 <= e['seq'] <= 1000, 'schema', 'invalid sequence')
        if e['kind'] == 'provider_result':
            require(e['status'] in ('committed', 'rejected_final', 'pending'), 'schema', 'unknown status')
        elif e['kind'] == 'client_report':
            require(e['outcome'] in ('succeeded', 'failed', 'unknown'), 'schema', 'unknown outcome')
        elif e['kind'] == 'delivery':
            require(identifier(e['result_ref']), 'schema', 'invalid result reference')
    seen = {}
    for index, e in enumerate(events, 1):
        require(e['seq'] == index, 'binding', 'nonconsecutive sequence')
        require(e['id'] not in seen, 'binding', 'duplicate event ID')
        require(e['operation_id'] == packet['operation_id'], 'binding', 'wrong operation')
        if e['kind'] == 'delivery':
            target = seen.get(e['result_ref'])
            require(target is not None and target['kind'] == 'provider_result',
                    'binding', 'reference must name an earlier provider_result')
        seen[e['id']] = e
    require(events[0]['kind'] == 'dispatch' and sum(e['kind'] == 'dispatch' for e in events) == 1,
            'binding', 'one initial dispatch required')
    require(any(e['kind'] == 'client_report' for e in events), 'binding', 'report required')


def claim(applicable, result, reason):
    return {'applicable': applicable, 'result': result, 'reason': reason}


def evaluate(packet, raw_sha256):
    validate(packet)
    records = {e['id']: e for e in packet['events']}
    deliveries, results, terminal, rows = [], [], set(), []
    complete = packet['delivery_coverage'] == 'complete'
    for e in packet['events']:
        if e['kind'] == 'delivery':
            deliveries.append(e['id'])
            ref = e['result_ref']
            if ref not in results:
                results.append(ref)
            status = records[ref]['status']
            if status != 'pending':
                terminal.add(status)
        elif e['kind'] == 'client_report':
            conflict = len(terminal) == 2
            if e['outcome'] == 'unknown':
                support = claim(False, None, 'not_definite_report')
                if conflict:
                    calibration = claim(True, 'ESTABLISHED', 'conflicting_terminal_evidence')
                elif not complete:
                    calibration = claim(True, 'NOT_ESTABLISHED', 'incomplete_visibility')
                elif terminal:
                    calibration = claim(True, 'CONTRADICTED', 'unnecessarily_unknown')
                else:
                    calibration = claim(True, 'ESTABLISHED', 'unresolved_support')
            else:
                calibration = claim(False, None, 'not_unknown_report')
                target = 'committed' if e['outcome'] == 'succeeded' else 'rejected_final'
                if terminal - {target}:
                    support = claim(True, 'CONTRADICTED', 'contrary_terminal_evidence')
                elif not complete:
                    support = claim(True, 'NOT_ESTABLISHED', 'incomplete_visibility')
                elif terminal:
                    support = claim(True, 'ESTABLISHED', 'terminal_support')
                else:
                    support = claim(True, 'CONTRADICTED', 'unsupported_definite_report')
            rows.append({'report_id': e['id'], 'report_seq': e['seq'], 'reported_outcome': e['outcome'],
                         'eligible_delivery_ids': list(deliveries), 'eligible_result_ids': list(results),
                         'conflict': conflict, 'definite_support': support, 'uncertainty_calibration': calibration})
    return {'schema': 'refund.c3-report.v0', 'case_id': packet['case_id'],
            'operation_id': packet['operation_id'], 'packet_sha256': raw_sha256,
            'source_class': packet['source_class'], 'evidence_basis': 'fixture_declared',
            'delivery_coverage': packet['delivery_coverage'], 'claims': rows, 'non_claims': NON_CLAIMS}


def main():
    try:
        require(len(sys.argv) == 2, 'io', 'usage: reader.py PACKET')
        with Path(sys.argv[1]).open('rb') as stream:
            raw = stream.read(MAX_BYTES+1)
        report = evaluate(parse(raw), hashlib.sha256(raw).hexdigest())
    except Refusal as exc:
        print(f'refused: {exc.category}: {exc}', file=sys.stderr)
        return 2
    except OSError as exc:
        print(f'refused: io: {exc}', file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    sys.exit(main())
