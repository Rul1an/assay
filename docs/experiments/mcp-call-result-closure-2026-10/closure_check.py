"""Offline checker for the normalized registered-action-closure.v0 contract."""
import hashlib
import json
import re
import sys
from pathlib import Path

PROFILE = 'registered-action-closure.v0'
PRIORITY = ('invalid_input', 'unsupported_profile', 'binding_mismatch', 'ambiguous_selection')
COMMON = {'seq', 'session', 'action_id', 'attempt', 'kind'}
BLOB = {'seq', 'session', 'action_id', 'attempt', 'state'}
STATES = {'transport': {'accepted', 'timeout', 'cancel_requested'},
          'observation': {'completed', 'failed', 'pending'}}
TERMINAL = {'completed', 'failed'}


def strict_json(raw):
    # Reuses the small duplicate/nonfinite rejection idiom from refund-c3's consumer.
    def pairs(items):
        obj = {}
        for key, value in items:
            if key in obj:
                raise ValueError('duplicate member')
            obj[key] = value
        return obj

    def constant(_):
        raise ValueError('nonfinite constant')

    return json.loads(raw, object_pairs_hook=pairs, parse_constant=constant)


def text(value):
    return isinstance(value, str) and bool(value)


def integer(value, minimum=1):
    return type(value) is int and value >= minimum


def digest(value):
    return isinstance(value, str) and re.fullmatch('[0-9a-f]{64}', value) is not None


def sha(value):
    return hashlib.sha256(value.encode('utf-8')).hexdigest()


def identity(row):
    return row['session'], row['action_id'], row['attempt']


def check(raw):
    """Consume one JSON document's bytes; return an outcome, never execution proof."""
    refusals, native, selected = [], [], []
    claim = {}

    def refuse(reason, where):
        item = {'reason': reason, 'where': where}
        if item not in refusals:
            refusals.append(item)

    def finish(outcome, reason):
        return {'profile': PROFILE, 'outcome': outcome, 'reason': reason,
                'session': claim.get('session'), 'cutoff_seq': claim.get('cutoff_seq'),
                'selection': selected, 'native': native,
                'refusals': sorted(refusals, key=lambda r: (PRIORITY.index(r['reason']), r['where']))}

    def shape(obj, fields, where):
        good = isinstance(obj, dict) and set(obj) == set(fields)
        if not good:
            refuse('invalid_input', where)
        return good

    def fields(obj, predicates, where):
        good = True
        for name, predicate in predicates.items():
            if name not in obj or not predicate(obj[name]):
                refuse('invalid_input', where + '.' + name)
                good = False
        return good

    def token(value, choices, where):
        if not text(value):
            refuse('invalid_input', where)
        elif value not in choices:
            refuse('unsupported_profile', where)

    def channels(value):
        return isinstance(value, list) and all(text(v) for v in value) and len(set(value)) == len(value)

    try:
        obj = strict_json(raw.decode('utf-8'))
        # Reject escaped lone surrogates anywhere, including otherwise unused values.
        json.dumps(obj, ensure_ascii=False).encode('utf-8')
    except (ValueError, UnicodeError, RecursionError):
        refuse('invalid_input', 'input.json')
        return finish('no_claim', 'invalid_input')
    if not isinstance(obj, dict):
        refuse('invalid_input', 'input')
        return finish('no_claim', 'invalid_input')
    shape(obj, {'profile', 'claim', 'context', 'trace_utf8', 'blobs'}, 'input')
    token(obj.get('profile'), {PROFILE}, 'profile')
    claim = obj.get('claim') if isinstance(obj.get('claim'), dict) else {}
    ctx = obj.get('context') if isinstance(obj.get('context'), dict) else {}
    shape(claim, {'kind', 'session', 'cutoff_seq', 'selection'}, 'claim')
    token(claim.get('kind'), {'all_selected_actions_terminal'}, 'claim.kind')
    claim_ok = fields(claim, {'session': text, 'cutoff_seq': integer}, 'claim')
    selected = claim.get('selection', [])
    selection_ok = isinstance(selected, list) and bool(selected)
    if not selection_ok:
        refuse('invalid_input', 'claim.selection')
    else:
        for i, entry in enumerate(selected):
            where = f'selection[{i}]'
            valid = shape(entry, {'action_id', 'attempt'}, where)
            if isinstance(entry, dict):
                valid = fields(entry, {'action_id': text, 'attempt': integer}, where) and valid
            selection_ok = valid and selection_ok
    shape(ctx, {'session', 'trace_sha256', 'source_class', 'continuity', 'coverage_through_seq',
                'allowed_channels', 'observed_channels'}, 'context')
    context_ok = fields(ctx, {'session': text, 'trace_sha256': digest, 'source_class': text,
                             'continuity': lambda v: type(v) is bool,
                             'coverage_through_seq': lambda v: integer(v, 0),
                             'allowed_channels': channels, 'observed_channels': channels}, 'context')
    trace = obj.get('trace_utf8')
    parsed = []
    if not isinstance(trace, str):
        refuse('invalid_input', 'trace_utf8')
    else:
        if digest(ctx.get('trace_sha256')) and sha(trace) != ctx['trace_sha256']:
            refuse('binding_mismatch', 'trace_sha256')
        if trace and not trace.endswith('\n'):
            refuse('invalid_input', 'trace.final_lf')
        for i, line in enumerate(trace.split('\n')[:-1] if trace.endswith('\n') else trace.split('\n') if trace else []):
            where = f'trace[{i}]'
            try:
                row = strict_json(line)
            except (ValueError, RecursionError):
                refuse('invalid_input', where)
                continue
            if not isinstance(row, dict):
                refuse('invalid_input', where)
                continue
            native.append(row)
            kind = row.get('kind')
            token(kind, {'register', 'transport', 'observation', 'producer_claim'}, where + '.kind')
            extra = {'register': set(), 'transport': {'state'}, 'observation': {'state', 'evidence_sha256'},
                     'producer_claim': {'claim_scope', 'declared_absence'}}
            if isinstance(kind, str) and kind in extra:
                shape(row, COMMON | extra[kind], where)
            good = fields(row, {'seq': integer, 'session': text, 'action_id': text, 'attempt': integer}, where)
            if kind in ('transport', 'observation'):
                token(row.get('state'), STATES[kind], where + '.state')
            if kind == 'observation':
                fields(row, {'evidence_sha256': digest}, where)
            if kind == 'producer_claim':
                token(row.get('claim_scope'), {'review_cycle'}, where + '.claim_scope')
                fields(row, {'declared_absence': lambda v: type(v) is bool}, where)
            if good:
                parsed.append(row)
    blobs = obj.get('blobs')
    retained = {}
    if not isinstance(blobs, dict):
        refuse('invalid_input', 'blobs')
    else:
        for key, value in blobs.items():
            where = 'blob:' + key
            if not digest(key) or not isinstance(value, str):
                refuse('invalid_input', where)
                continue
            if sha(value) != key:
                refuse('binding_mismatch', where + '.digest')
            try:
                blob = strict_json(value)
            except (ValueError, RecursionError):
                refuse('invalid_input', where)
                continue
            if shape(blob, BLOB, where):
                fields(blob, {'seq': integer, 'session': text, 'action_id': text, 'attempt': integer}, where)
                token(blob.get('state'), STATES['observation'], where + '.state')
                retained[key] = blob

    sessions_agree = (text(claim.get('session')) and text(ctx.get('session')) and
                      claim['session'] == ctx['session'] and all(r['session'] == claim['session'] for r in parsed))
    if text(claim.get('session')) and text(ctx.get('session')) and not sessions_agree:
        refuse('binding_mismatch', 'sessions')
    registered, terminal_seen = {}, set()
    last_seq = 0
    for row in parsed:
        ident = identity(row)
        where = f'trace.seq:{row["seq"]}'
        if row['seq'] <= last_seq:
            refuse('invalid_input', where + '.order')
        last_seq = row['seq']
        if row.get('kind') == 'register':
            if ident in registered:
                refuse('ambiguous_selection', where + '.registration')
            else:
                registered[ident] = row['seq']
        elif ident not in registered or registered[ident] >= row['seq']:
            refuse('invalid_input', where + '.orphan')
        if row.get('kind') == 'observation':
            state = row.get('state')
            if isinstance(state, str) and state in STATES['observation']:
                if ident in terminal_seen:
                    refuse('invalid_input', where + '.history')
                if state in TERMINAL:
                    terminal_seen.add(ident)
            key = row.get('evidence_sha256')
            if isinstance(key, str) and key in retained and any(retained[key].get(k) != row.get(k) for k in BLOB):
                refuse('binding_mismatch', where + '.blob')
    identities = []
    if claim_ok and selection_ok:
        identities = [(claim['session'], e['action_id'], e['attempt']) for e in selected]
        if len(set(identities)) != len(identities):
            refuse('ambiguous_selection', 'selection')
        for ident in identities:
            if sessions_agree and ident not in registered:
                refuse('invalid_input', f'selection:{ident}.missing_registration')
            if ident in registered and registered[ident] > claim['cutoff_seq']:
                refuse('invalid_input', f'selection:{ident}.late_registration')
    if refusals:
        reason = min((r['reason'] for r in refusals), key=PRIORITY.index)
        return finish('no_claim', reason)
    # Well-typed, bound input from here on. Missing blobs are not refusals.
    assert context_ok and claim_ok and selection_ok
    if ctx['source_class'] != 'local-supervisor':
        return finish('not_established', 'source_unaccepted')
    observations = [r for r in parsed if r['kind'] == 'observation' and
                    identity(r) in identities and r['seq'] <= claim['cutoff_seq']]
    admissible = [r for r in observations if r['evidence_sha256'] in retained]
    if any(r['state'] == 'pending' and r['seq'] == claim['cutoff_seq'] for r in admissible):
        return finish('contradicted', 'pending_at_cutoff')
    if (not ctx['continuity'] or ctx['coverage_through_seq'] < claim['cutoff_seq'] or
        not ctx['allowed_channels'] or not set(ctx['allowed_channels']) <= set(ctx['observed_channels'])):
        return finish('not_established', 'coverage_incomplete')
    if len(admissible) != len(observations):
        return finish('not_established', 'evidence_missing')
    completed = {identity(r) for r in admissible if r['state'] in TERMINAL}
    if not set(identities) <= completed:
        return finish('not_established', 'terminal_not_observed')
    return finish('supported', 'all_selected_terminal')


def main(argv):
    if len(argv) != 1:
        print('usage: closure_check.py INPUT.json', file=sys.stderr)
        return 64
    try:
        raw = Path(argv[0]).read_bytes()
    except OSError as exc:
        print(f'cannot read input: {exc}', file=sys.stderr)
        return 66
    print(json.dumps(check(raw), sort_keys=True, ensure_ascii=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv[1:]))
