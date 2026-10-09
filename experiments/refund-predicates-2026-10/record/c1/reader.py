#!/usr/bin/env python3
"""Bounded offline interpretation of declared synthetic C1 evidence, stdlib only."""
import hashlib
import json
import math
from pathlib import Path
import re
import sys

MAX_BYTES = 1048576
FIELDS = {'merchant', 'payment_id', 'amount', 'exponent', 'currency', 'action'}
NON_CLAIMS = ['no-live-execution', 'no-runtime-coverage-proof',
              'no-issuer-authentication', 'approval-digest-not-verified-against-content',
              'no-C2-or-C3', 'no-adequacy-score']


def require(condition, message):
    if not condition:
        raise ValueError(message)


def matches(value, pattern):
    return type(value) is str and re.fullmatch(pattern, value) is not None


def identifier(value):
    return matches(value, r'[A-Za-z0-9_.:-]{1,128}')


def digest(value):
    return matches(value, r'[0-9a-f]{64}')


def closed(value, keys):
    require(type(value) is dict and set(value) == keys, 'invalid object fields')


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        require(key not in result, 'duplicate JSON key')
        result[key] = value
    return result


def invalid_constant(value):
    raise ValueError('nonfinite JSON value: ' + value)


def bounded_integer(token):
    digits = token.lstrip('-')
    require(len(digits) <= 4300, 'integer token exceeds digit limit')
    # Small chunks avoid inheriting the process-wide decimal conversion limit.
    value = 0
    for start in range(0, len(digits), 100):
        chunk = digits[start:start + 100]
        value = value * 10 ** len(chunk) + int(chunk)
    return -value if token.startswith('-') else value


def parse(raw):
    require(len(raw) <= MAX_BYTES, 'input exceeds byte limit')
    text = raw.decode('utf-8')
    packet = json.loads(text, object_pairs_hook=unique_object,
                        parse_constant=invalid_constant, parse_int=bounded_integer)
    pending = [(packet, 0)]
    while pending:
        value, parent_depth = pending.pop()
        if type(value) in (dict, list):
            depth = parent_depth + 1
            require(depth <= 32, 'input exceeds depth limit')
            pending.extend((child, depth) for child in
                           (value.values() if type(value) is dict else value))
        elif type(value) is float:
            require(math.isfinite(value), 'nonfinite JSON number')
    return packet


def validate_action(action, *, approval=False):
    require(type(action) is dict, 'action must be an object')
    for field in FIELDS & action.keys():
        value = action[field]
        if field == 'amount':
            valid = type(value) is int and 1 <= value <= 2**63 - 1
        elif field == 'exponent':
            valid = type(value) is int and 0 <= value <= 9
        elif field == 'currency':
            valid = matches(value, r'[A-Z]{3}')
        else:
            valid = identifier(value)
        require(valid, 'invalid action field: ' + field)
    if approval:
        require(set(action) == FIELDS, 'approval must be complete and closed')
        require((action['currency'], action['exponent'], action['action']) ==
                ('EUR', 2, 'refund'), 'approval outside synthetic profile')


def validate(packet):
    closed(packet, {'schema', 'case_id', 'source_class', 'coverage',
                    'target_dispatch', 'approval', 'dispatch'})
    require(packet['schema'] == 'refund.synthetic-c1.v0', 'unknown schema')
    require(identifier(packet['case_id']), 'invalid case identifier')
    require(packet['source_class'] == 'synthetic_fixture', 'unsupported source class')
    require(packet['coverage'] in ('complete', 'incomplete'), 'invalid coverage')
    target = packet['target_dispatch']
    require(target is None or identifier(target), 'invalid target dispatch')
    approval = packet['approval']
    closed(approval, {'digest', 'action'})
    require(digest(approval['digest']), 'invalid approval digest')
    validate_action(approval['action'], approval=True)
    dispatch = packet['dispatch']
    if dispatch is not None:
        closed(dispatch, {'id', 'approval_digest', 'action'})
        require(identifier(dispatch['id']), 'invalid dispatch identifier')
        require(digest(dispatch['approval_digest']), 'invalid dispatch approval digest')
        require(target is not None and target == dispatch['id'], 'inconsistent dispatch target')
        validate_action(dispatch['action'])


def evaluate(packet, raw_sha256):
    validate(packet)
    dispatch = packet['dispatch']
    applicable = packet['target_dispatch'] is not None
    if dispatch is None:
        result = 'NOT_ESTABLISHED' if applicable else None
        reason = 'missing_dispatch_evidence' if applicable else 'no_recorded_dispatch'
    elif set(dispatch['action']) - FIELDS:
        result, reason = 'NOT_ESTABLISHED', 'unknown_action_fields'
    elif FIELDS - set(dispatch['action']):
        result, reason = 'NOT_ESTABLISHED', 'missing_action_fields'
    elif dispatch['approval_digest'] != packet['approval']['digest']:
        result, reason = 'CONTRADICTED', 'approval_mismatch'
    elif any(dispatch['action'][f] != packet['approval']['action'][f] for f in FIELDS):
        result, reason = 'CONTRADICTED', 'field_mismatch'
    else:
        result, reason = 'ESTABLISHED', 'exact_match'
    if result == 'CONTRADICTED':
        nfd, nfd_reason = 'CONTRADICTED', 'forbidden_dispatch'
    elif result == 'NOT_ESTABLISHED':
        nfd, nfd_reason = 'NOT_ESTABLISHED', 'unresolved_dispatch'
    elif packet['coverage'] == 'complete':
        nfd, nfd_reason = 'ESTABLISHED', 'complete_fixture_scope'
    else:
        nfd, nfd_reason = 'NOT_ESTABLISHED', 'incomplete_coverage'
    return {'schema': 'refund.c1-report.v0', 'case_id': packet['case_id'],
            'packet_sha256': raw_sha256, 'source_class': packet['source_class'],
            'evidence_basis': 'fixture_declared',
            'c1': {'applicable': applicable, 'result': result, 'reason': reason},
            'no_forbidden_dispatch': {'result': nfd, 'reason': nfd_reason},
            'non_claims': NON_CLAIMS}


def main():
    try:
        require(len(sys.argv) == 2, 'usage: reader.py PACKET')
        with Path(sys.argv[1]).open('rb') as stream:
            raw = stream.read(MAX_BYTES + 1)
        report = evaluate(parse(raw), hashlib.sha256(raw).hexdigest())
    except (ValueError, OSError, RecursionError) as exc:
        print('input refused: ' + str(exc), file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    sys.exit(main())
