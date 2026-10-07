"""Independent, stdlib-only C2 byte reader and grouped-history reproducer.

The input parser is local and does not use a JSON decoder. Numeric tokens retain
representation until schema validation; report serialization alone uses json.
"""
import hashlib
import json
import re
import sys
from collections import defaultdict
from dataclasses import dataclass
from itertools import groupby


class Refusal(Exception):
    def __init__(self, category, detail):
        super().__init__(detail)
        self.category = category


def require(condition, category, detail):
    if not condition:
        raise Refusal(category, detail)


@dataclass(frozen=True)
class Number:
    spelling: str

    @property
    def integral(self):
        return not any(c in self.spelling for c in '.eE')

    def integer(self):
        # No ambient interpreter decimal-digit conversion limit participates.
        digits = self.spelling.lstrip('-')
        result = 0
        for offset in range(0, len(digits), 9):
            part = digits[offset:offset + 9]
            result = result * 10 ** len(part) + int(part)
        return -result if self.spelling.startswith('-') else result


class TextParser:
    """Recursive descent with bounded container depth and decoded-key uniqueness."""
    NUMBER = re.compile(r'-?(?:0|[1-9][0-9]*)(?:\.[0-9]+)?(?:[eE][+-]?[0-9]+)?')

    def __init__(self, text):
        self.text = text
        self.pos = 0

    def space(self):
        while self.pos < len(self.text) and self.text[self.pos] in ' \t\r\n':
            self.pos += 1

    def take(self, token):
        self.space()
        if self.text.startswith(token, self.pos):
            self.pos += len(token)
            return True
        return False

    def string(self):
        require(self.take('"'), 'parse', 'string expected')
        chars = []
        escapes = {'"': '"', '\\': '\\', '/': '/', 'b': '\b', 'f': '\f',
                   'n': '\n', 'r': '\r', 't': '\t'}
        while self.pos < len(self.text):
            char = self.text[self.pos]
            self.pos += 1
            if char == '"':
                # Decode surrogate pairs to the same string as literal UTF-8.
                joined = []
                index = 0
                while index < len(chars):
                    code = ord(chars[index])
                    if (0xD800 <= code <= 0xDBFF and index + 1 < len(chars)
                            and 0xDC00 <= ord(chars[index + 1]) <= 0xDFFF):
                        joined.append(chr(0x10000 + (code - 0xD800) * 1024
                                          + ord(chars[index + 1]) - 0xDC00))
                        index += 2
                    else:
                        joined.append(chars[index])
                        index += 1
                return ''.join(joined)
            require(ord(char) >= 32, 'parse', 'unescaped control')
            if char == '\\':
                require(self.pos < len(self.text), 'parse', 'unfinished escape')
                char = self.text[self.pos]
                self.pos += 1
                if char == 'u':
                    digits = self.text[self.pos:self.pos + 4]
                    require(re.fullmatch(r'[0-9a-fA-F]{4}', digits) is not None,
                            'parse', 'invalid unicode escape')
                    chars.append(chr(int(digits, 16)))
                    self.pos += 4
                else:
                    require(char in escapes, 'parse', 'unknown escape')
                    chars.append(escapes[char])
            else:
                chars.append(char)
        raise Refusal('parse', 'unfinished string')

    def value(self, enclosing_depth=0):
        self.space()
        require(self.pos < len(self.text), 'parse', 'value expected')
        char = self.text[self.pos]
        if char in '{[':
            depth = enclosing_depth + 1
            require(depth <= 32, 'parse', 'depth exceeds 32')
            self.pos += 1
            if char == '{':
                result = {}
                if self.take('}'):
                    return result
                while True:
                    key = self.string()
                    require(key not in result, 'parse', 'duplicate object key')
                    require(self.take(':'), 'parse', 'colon expected')
                    result[key] = self.value(depth)
                    if self.take('}'):
                        return result
                    require(self.take(','), 'parse', 'comma expected')
            result = []
            if self.take(']'):
                return result
            while True:
                result.append(self.value(depth))
                if self.take(']'):
                    return result
                require(self.take(','), 'parse', 'comma expected')
        if char == '"':
            return self.string()
        for token, value in [('true', True), ('false', False), ('null', None)]:
            if self.take(token):
                return value
        match = self.NUMBER.match(self.text, self.pos)
        require(match is not None, 'parse', 'invalid value')
        self.pos = match.end()
        value = Number(match.group())
        require(not value.integral or len(value.spelling.lstrip('-')) <= 4300,
                'parse', 'integer exceeds 4300 digits')
        return value

    def document(self):
        value = self.value()
        self.space()
        require(self.pos == len(self.text), 'parse', 'trailing data')
        return value


def read_packet(path):
    try:
        with open(path, 'rb') as stream:
            raw = stream.read(1048577)
    except (OSError, ValueError) as error:
        raise Refusal('io', 'cannot read packet') from error
    require(len(raw) <= 1048576, 'parse', 'packet exceeds byte limit')
    try:
        text = raw.decode('utf-8')
    except UnicodeDecodeError as error:
        raise Refusal('parse', 'invalid UTF-8') from error
    return raw, TextParser(text).document()


def shape(value, keys):
    require(type(value) is dict and set(value) == set(keys.split()),
            'schema', 'closed object shape')


def syntax(value, pattern):
    require(type(value) is str and re.fullmatch(pattern, value) is not None,
            'schema', 'string syntax')


def identifier(value):
    syntax(value, r'[A-Za-z0-9_.:-]{1,128}')


def digest(value):
    syntax(value, r'[0-9a-f]{64}')


def one_of(value, options):
    require(type(value) is str and value in options, 'schema', 'string enum')


def integer(value):
    require(type(value) is Number and value.integral, 'schema', 'integer token required')
    return value.integer()


def validate_schema(packet):
    shape(packet, 'schema case_id source_class custody effect_coverage admissions events')
    one_of(packet['schema'], ('refund.synthetic-c2.v0',))
    one_of(packet['source_class'], ('synthetic_fixture',))
    identifier(packet['case_id'])
    identifier(packet['custody'])
    one_of(packet['effect_coverage'], ('complete', 'incomplete'))
    for field, maximum in [('admissions', 100), ('events', 1000)]:
        require(type(packet[field]) is list and 1 <= len(packet[field]) <= maximum,
                'schema', 'array count')
    for admission in packet['admissions']:
        shape(admission, 'admission_id approval_digest operation_id reissue_of')
        identifier(admission['admission_id'])
        identifier(admission['operation_id'])
        digest(admission['approval_digest'])
        if admission['reissue_of'] is not None:
            identifier(admission['reissue_of'])
    for event in packet['events']:
        require(type(event) is dict, 'schema', 'event object required')
        one_of(event.get('kind'), ('dispatch', 'observation', 'epoch_close'))
        kind = event['kind']
        if kind == 'dispatch':
            shape(event, 'id seq kind approval_digest workflow_key')
            if event['approval_digest'] is not None:
                digest(event['approval_digest'])
            if event['workflow_key'] is not None:
                identifier(event['workflow_key'])
        elif kind == 'observation':
            shape(event, 'id seq kind effect_id dispatch_id amount currency status')
            identifier(event['effect_id'])
            identifier(event['dispatch_id'])
            require(1 <= integer(event['amount']) <= 2**63 - 1, 'schema', 'amount range')
            syntax(event['currency'], r'[A-Z]{3}')
            one_of(event['status'], ('pending', 'committed', 'failed', 'reversed'))
        else:
            shape(event, 'id seq kind closer')
            identifier(event['closer'])
        identifier(event['id'])
        integer(event['seq'])


def bind_histories(packet):
    """Validate identities, then group all observations before evaluating paths."""
    admissions = packet['admissions']
    require(len({a['admission_id'] for a in admissions}) == len(admissions),
            'binding', 'duplicate admission id')
    require(len({a['approval_digest'] for a in admissions}) == len(admissions),
            'binding', 'duplicate approval digest')
    prior = {}
    operation_seen = set()
    for admission in admissions:
        operation = admission['operation_id']
        reissue = admission['reissue_of']
        require((operation not in operation_seen and reissue is None)
                or (operation in operation_seen and reissue in prior and prior[reissue] == operation),
                'binding', 'invalid admission reissue')
        prior[admission['admission_id']] = operation
        operation_seen.add(operation)
    events = packet['events']
    require(len({event['id'] for event in events}) == len(events), 'binding', 'duplicate event id')
    require(all(event['seq'].integer() == index for index, event in enumerate(events, 1)),
            'binding', 'noncontiguous seq')
    positions = {event['id']: index for index, event in enumerate(events)}
    dispatches = {event['id']: event for event in events if event['kind'] == 'dispatch'}
    closes = [index for index, event in enumerate(events) if event['kind'] == 'epoch_close']
    require(len(closes) <= 1, 'binding', 'multiple epoch closures')
    close = closes[0] if closes else None
    histories = defaultdict(list)
    for index, event in enumerate(events):
        if event['kind'] == 'observation':
            dispatch_id = event['dispatch_id']
            require(dispatch_id in dispatches and positions[dispatch_id] < index,
                    'binding', 'observation needs earlier dispatch')
            histories[event['effect_id']].append((index, event))
    for history in histories.values():
        signatures = {(event['dispatch_id'], event['amount'].integer(), event['currency'])
                      for _, event in history}
        require(len(signatures) == 1, 'binding', 'conflicting immutable effect fields')
        path = [status for status, _ in groupby(event['status'] for _, event in history)]
        # Derive legality from terminal suffixes: remove optional pending, then
        # optional committed; only a singleton failed OR reversed suffix remains.
        rest = path[:]
        if rest and rest[0] == 'pending':
            rest.pop(0)
        had_commit = bool(rest and rest[0] == 'committed')
        if had_commit:
            rest.pop(0)
        require(not rest or rest == ['reversed'] or (not had_commit and rest == ['failed']),
                'binding', 'invalid collapsed lifecycle')
    return dispatches, dict(histories), close


def reproduce(packet, raw):
    validate_schema(packet)
    dispatches, histories, close = bind_histories(packet)
    admitted = {a['approval_digest']: a['operation_id'] for a in packet['admissions']}
    effects = {}
    closure_contradicted = False
    for effect_id, history in histories.items():
        statuses = {event['status'] for _, event in history}
        dispatch = dispatches[history[0][1]['dispatch_id']]
        counting_positions = [index for index, event in history
                              if event['status'] in ('committed', 'reversed')]
        if close is not None:
            before = [event['status'] for index, event in history if index < close]
            after = [event['status'] for index, event in history if index > close]
            closure_contradicted |= bool(after) and (not before or any(s != before[-1] for s in after))
        effects[effect_id] = {
            'operation': admitted.get(dispatch['approval_digest']),
            'key': dispatch['workflow_key'],
            'counted': bool(counting_positions),
            'pending': history[-1][1]['status'] == 'pending',
            'reversed': 'reversed' in statuses,
            'unobserved': 'reversed' in statuses and 'committed' not in statuses,
            'late': bool(counting_positions) and close is not None and counting_positions[0] > close,
        }
    unattributed = {eid: effect for eid, effect in effects.items() if effect['operation'] is None}
    blocking = sorted(eid for eid, effect in unattributed.items() if effect['counted'] or effect['pending'])
    rows = []
    for operation in sorted(set(admitted.values())):
        owned = {eid: effect for eid, effect in effects.items() if effect['operation'] == operation}
        committed = sorted(eid for eid, effect in owned.items() if effect['counted'])
        keys = sorted({d['workflow_key'] for d in dispatches.values()
                       if admitted.get(d['approval_digest']) == operation and d['workflow_key'] is not None})
        if len(committed) >= 2:
            result, reason = 'CONTRADICTED', 'multiple_committed_effects'
        else:
            blockers = [
                (closure_contradicted, 'closure_contradicted'),
                (close is None, 'epoch_open'),
                (packet['effect_coverage'] == 'incomplete', 'incomplete_effect_coverage'),
                (bool(blocking), 'unattributed_effect_in_scope'),
                (any(effect['pending'] for effect in owned.values()), 'pending_at_closure'),
            ]
            reason = next((reason for applies, reason in blockers if applies), 'at_most_one_committed')
            result = 'ESTABLISHED' if reason == 'at_most_one_committed' else 'NOT_ESTABLISHED'
        rows.append({
            'operation_id': operation, 'result': result, 'reason': reason,
            'committed_effect_ids': committed,
            'late_effect_ids': sorted(eid for eid, effect in owned.items() if effect['late']),
            'commit_unobserved_effect_ids': sorted(eid for eid, effect in owned.items() if effect['unobserved']),
            'reversed_effect_ids': sorted(eid for eid, effect in owned.items() if effect['reversed']),
            'workflow_keys': keys,
            'key_hint_effect_ids': sorted(eid for eid, effect in unattributed.items() if effect['key'] in keys),
        })
    return {
        'schema': 'refund.c2-report.v0', 'case_id': packet['case_id'],
        'packet_sha256': hashlib.sha256(raw).hexdigest(),
        'source_class': packet['source_class'], 'custody': packet['custody'],
        'evidence_basis': 'fixture_declared', 'effect_coverage': packet['effect_coverage'],
        'epoch': 'open' if close is None else 'closed',
        'closure_contradicted': closure_contradicted,
        'unattributed_blocking_effect_ids': blocking, 'operations': rows,
        'non_claims': ['no-live-execution', 'no-provider-semantics', 'no-exactly-once',
                       'no-refund-safety', 'no-custody-authentication',
                       'no-runtime-coverage-proof', 'no-adequacy-score'],
    }


def main():
    try:
        require(len(sys.argv) == 2, 'io', 'expected one packet path')
        raw, packet = read_packet(sys.argv[1])
        report = reproduce(packet, raw)
    except Refusal as error:
        print(f'refused: {error.category}: {error}', file=sys.stderr)
        return 2
    print(json.dumps(report, sort_keys=True, separators=(',', ':')))
    return 0


if __name__ == '__main__':
    sys.exit(main())
