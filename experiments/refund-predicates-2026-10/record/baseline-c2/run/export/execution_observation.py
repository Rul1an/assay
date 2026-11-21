"""Pure, closed codecs for process execution evidence; never scores or executes."""
from __future__ import annotations

import hashlib
import json
import math
import re

PREFIX_SCHEMA = 'corpus-adequacy.execution-observation-prefix.v0'
ADMISSION_SCHEMA = 'corpus-adequacy.execution-admission.v0'
FINAL_SCHEMA = 'corpus-adequacy.execution-observation.v0'
SCHEMAS = {'prefix': PREFIX_SCHEMA, 'admission': ADMISSION_SCHEMA, 'final': FINAL_SCHEMA}
MAX_BYTES = 8 * 1024 * 1024
MAX_DEPTH = 64
NON_CLAIMS = ['no-adequacy-score', 'no-policy-authentication', 'no-global-replay-prevention']
REASONS = frozenset({'operator-prerequisite-refused', 'build-failed', 'anchor-mismatch',
                     'source-drift', 'restoration-failed', 'cleanup-failed', 'timeout',
                     'output-cap', 'signal', 'unproved', 'incomplete', 'unexpected-exit', 'parse-error',
                     'selector-missing', 'backend-failed', 'interrupted', 'policy-refused',
                     'evidence-incomplete', 'binding-mismatch', 'operator-refused'})
ADMISSION_REASONS = frozenset({'policy-refused', 'evidence-incomplete', 'binding-mismatch', 'operator-refused'})
_DIGEST = re.compile(r'sha256:[0-9a-f]{64}\Z')
_COMMIT = re.compile(r'[0-9a-f]{40}\Z')


def _fail(message):
    raise ValueError(message)


def _object(value, keys, where):
    if type(value) is not dict or set(value) != set(keys.split()):
        _fail(where + ': expected closed object fields ' + keys)
    return value


def _text(value, where):
    if type(value) is not str or not value or len(value) > 4096:
        _fail(where + ': expected bounded nonempty string')


def _digest(value):
    if type(value) is not str or not _DIGEST.fullmatch(value):
        _fail('invalid SHA-256 binding')


def _integer(value, where, minimum=0):
    if type(value) is not int or value < minimum:
        _fail(where + ': invalid integer')


def _strings(value, where, *, empty=True):
    if type(value) is not list or (not empty and not value):
        _fail(where + ': expected array')
    for item in value:
        _text(item, where)
    if len(set(value)) != len(value):
        _fail(where + ': duplicate member')


def _json_tree(value, depth=0):
    if depth > MAX_DEPTH:
        _fail('JSON exceeds depth limit')
    if value is None or type(value) in (bool, int, str):
        return
    if type(value) is float:
        if not math.isfinite(value):
            _fail('non-finite number')
        return
    if type(value) is dict:
        for key, member in value.items():
            if type(key) is not str:
                _fail('object key must be string')
            _json_tree(member, depth + 1)
    elif type(value) is list:
        for member in value:
            _json_tree(member, depth + 1)
    else:
        _fail('not a JSON value')


def canonical_bytes(value):
    """Canonical JSON bytes; no artifact semantic claim is implied."""
    try:
        _json_tree(value)
        raw = (json.dumps(value, ensure_ascii=False, allow_nan=False,
                          sort_keys=True, indent=2) + '\n').encode('utf-8')
    except (RecursionError, UnicodeError, OverflowError) as exc:
        raise ValueError('invalid JSON value') from exc
    if len(raw) > MAX_BYTES:
        _fail('JSON exceeds byte limit')
    return raw


def sha256(raw):
    return 'sha256:' + hashlib.sha256(raw).hexdigest()


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            _fail('duplicate JSON key')
        result[key] = value
    return result


def _bindings(value):
    _object(value, 'tool_version tool_commit tool_content_sha256 tool_source_state manifest_sha256 '
            'corpus_sha256 source_sha256 execution_profile backend_sha256 environment_sha256 '
            'context_sha256 policy_identity interpreter_identity selectors exit_policy', 'bindings')
    for key in value:
        if key.endswith('_sha256') or key in ('policy_identity', 'interpreter_identity'):
            _digest(value[key])
    _text(value['tool_version'], 'tool version')
    if value['tool_source_state'] not in ('exact', 'dirty', 'unresolved'):
        _fail('invalid tool source state')
    commit = value['tool_commit']
    if value['tool_source_state'] == 'exact':
        if type(commit) is not str or not _COMMIT.fullmatch(commit):
            _fail('exact tool requires commit')
    elif commit is not None:
        _fail('non-exact tool cannot claim commit')
    if value['execution_profile'] not in ('trusted-local', 'contained-oci-v1'):
        _fail('invalid execution profile')
    selectors = _object(value['selectors'], 'outcome_from diagnostic_from', 'selectors')
    _strings(selectors['outcome_from'], 'outcome selectors', empty=False)
    if selectors['diagnostic_from'] is not None:
        _strings(selectors['diagnostic_from'], 'diagnostic selectors', empty=False)
        if set(selectors['outcome_from']) & set(selectors['diagnostic_from']):
            _fail('overlapping selectors')
    exits = _object(value['exit_policy'], 'accepted_exit_codes unproved_exit_codes', 'exit policy')
    for key, codes in exits.items():
        if type(codes) is not list or (key == 'accepted_exit_codes' and not codes):
            _fail('invalid exit codes')
        for code in codes:
            _integer(code, 'exit code')
            if code > 255:
                _fail('invalid exit code')
        if len(set(codes)) != len(codes):
            _fail('duplicate exit code')
    if set(exits['accepted_exit_codes']) & set(exits['unproved_exit_codes']):
        _fail('overlapping exit policy')


def _schedule(rows):
    if type(rows) is not list or len(rows) < 3:
        _fail('incomplete schedule')
    order = {'build': 0, 'baseline': 1, 'control': 2, 'ordinary': 3}
    last = -1
    ids = set()
    groups = {}
    for index, row in enumerate(rows):
        _object(row, 'step_id kind group label control_polarity vector_ids mutation_sha256', 'schedule row')
        _text(row['step_id'], 'step id')
        if row['step_id'] in ids:
            _fail('duplicate step')
        ids.add(row['step_id'])
        kind = row['kind']
        if type(kind) is not str or kind not in order or order[kind] < last:
            _fail('invalid schedule order')
        last = order[kind]
        _strings(row['vector_ids'], 'vectors', empty=kind == 'build')
        if kind == 'build':
            if index != 0 or row['vector_ids'] or any(row[k] is not None for k in ('group', 'label', 'control_polarity', 'mutation_sha256')):
                _fail('invalid build declaration')
        else:
            if rows[0]['kind'] != 'build':
                _fail('missing build')
            _text(row['group'], 'group')
            if kind == 'baseline':
                if row['group'] in groups or any(row[k] is not None for k in ('label', 'control_polarity', 'mutation_sha256')):
                    _fail('invalid baseline')
                groups[row['group']] = row['vector_ids']
            else:
                _text(row['label'], 'mutation label')
                _digest(row['mutation_sha256'])
                if groups.get(row['group']) != row['vector_ids']:
                    _fail('mutation vector set differs from baseline')
                if kind == 'control':
                    if row['control_polarity'] not in ('positive', 'inert'):
                        _fail('invalid control polarity')
                elif row['control_polarity'] is not None:
                    _fail('ordinary step cannot claim control polarity')
    if not groups or last != 3:
        _fail('missing baseline or ordinary step')


def _receipt(receipt, declaration, step, slot, invocations):
    _object(receipt, 'invocation_id step_id vector_id source_sha256 raw_sha256 raw_size evidence_sha256', 'receipt')
    _text(receipt['invocation_id'], 'invocation id')
    if receipt['invocation_id'] in invocations:
        _fail('reused invocation')
    invocations.add(receipt['invocation_id'])
    if (receipt['step_id'] != declaration['step_id'] or receipt['vector_id'] != slot['vector_id']
            or receipt['source_sha256'] != step['source_sha256']):
        _fail('receipt does not bind current invocation')
    for key in ('source_sha256', 'raw_sha256', 'evidence_sha256'):
        _digest(receipt[key])
    _integer(receipt['raw_size'], 'raw byte size')


def _reason(value):
    if type(value) is not str or value not in REASONS:
        _fail('unknown reason')


def _step(step, declaration, invocations):
    _object(step, 'step_id state source_sha256 application anchor_hits build_state restored preflight failure slots', 'step')
    if step['step_id'] != declaration['step_id']:
        _fail('step differs from schedule')
    if type(step['restored']) is not bool:
        _fail('restoration must be boolean')
    if step['anchor_hits'] is not None:
        _integer(step['anchor_hits'], 'anchor hits')
    if step['source_sha256'] is not None:
        _digest(step['source_sha256'])
    if step['application'] not in ('not_applicable', 'pending', 'applied', 'not_run', 'failed'):
        _fail('invalid application state')
    if step['build_state'] not in ('not_run', 'succeeded', 'failed'):
        _fail('invalid build state')
    preflight = step['preflight']
    if preflight is not None:
        _object(preflight, 'decision reason evidence_sha256', 'preflight')
        if declaration['kind'] != 'control':
            _fail('preflight only applies to controls')
        _digest(preflight['evidence_sha256'])
        if preflight['decision'] == 'proceed':
            if preflight['reason'] is not None:
                _fail('proceed cannot have stop reason')
        elif preflight['decision'] == 'stop':
            if preflight['reason'] != 'operator-prerequisite-refused':
                _fail('invalid preflight stop reason')
        else:
            _fail('invalid preflight decision')
    if step['failure'] is not None:
        _object(step['failure'], 'reason evidence_sha256', 'failure')
        _reason(step['failure']['reason']); _digest(step['failure']['evidence_sha256'])
    if type(step['slots']) is not list:
        _fail('slots must be array')
    if [slot.get('vector_id') if type(slot) is dict else None for slot in step['slots']] != declaration['vector_ids']:
        _fail('slots differ from declared vectors')
    states = []
    stopped_slot = False
    for slot in step['slots']:
        _object(slot, 'vector_id state outcome diagnostic selector_presence receipt reason evidence_sha256', 'slot')
        state = slot['state']; states.append(state)
        if stopped_slot and state != 'not_run':
            _fail('execution continued after stopped vector')
        if state in ('abnormal', 'not_run'):
            stopped_slot = True
        if state in ('observed', 'abnormal'):
            if state == 'abnormal' and slot['reason'] == 'interrupted' and slot['receipt'] is None:
                _digest(slot['evidence_sha256'])  # journal validator must bind exact dispatch
            else:
                _receipt(slot['receipt'], declaration, step, slot, invocations)
        elif slot['receipt'] is not None:
            _fail('unstarted slot cannot have receipt')
        if state == 'observed':
            presence = _object(slot['selector_presence'], 'outcome diagnostic', 'selector presence')
            if any(type(value) is not bool or not value for value in presence.values()):
                _fail('observed slot lacks required selector')
            if slot['reason'] is not None or slot['evidence_sha256'] is not None:
                _fail('observed slot cannot carry stop reason')
        else:
            if any(slot[key] is not None for key in ('outcome', 'diagnostic', 'selector_presence')):
                _fail('non-observed slot cannot carry parsed values')
            if state in ('abnormal', 'not_run'):
                _reason(slot['reason']); _digest(slot['evidence_sha256'])
            elif state == 'pending':
                if slot['reason'] is not None or slot['evidence_sha256'] is not None:
                    _fail('pending slot cannot have stop evidence')
            else:
                _fail('invalid slot state')
    state = step['state']
    if state == 'complete':
        if any(s != 'observed' for s in states) or not step['restored'] or step['build_state'] != 'succeeded' or step['failure'] is not None:
            _fail('incomplete completed step')
        if preflight is not None and preflight['decision'] == 'stop':
            _fail('stopped preflight cannot complete')
        _digest(step['source_sha256'])
        mutation = declaration['kind'] in ('control', 'ordinary')
        if step['application'] != ('applied' if mutation else 'not_applicable'):
            _fail('wrong application state')
        if step['anchor_hits'] != (1 if mutation else None):
            _fail('wrong anchor count')
    elif state in ('pending', 'not_run'):
        if any(s != state for s in states) or step['application'] != state or step['anchor_hits'] is not None or step['source_sha256'] is not None or step['build_state'] != 'not_run' or preflight is not None or step['failure'] is not None:
            _fail('unstarted step claims effects')
        if step['restored'] != (state == 'not_run'):
            _fail('unstarted restoration mismatch')
        if state == 'pending' and declaration['kind'] != 'ordinary':
            _fail('only ordinary steps can be pending')
    elif state == 'stopped':
        if 'pending' in states:
            _fail('stopped step has pending slots')
        if preflight is not None and preflight['decision'] == 'stop':
            if any(s != 'not_run' for s in states) or step['application'] != 'not_run' or step['source_sha256'] is not None or step['anchor_hits'] is not None or step['build_state'] != 'not_run' or not step['restored'] or step['failure'] is not None:
                _fail('pre-invocation stop contradicts effects')
        elif step['failure'] is None:
            _fail('stopped step has no failure or preflight stop')
    else:
        _fail('invalid step state')


def _observation(doc, kind):
    _object(doc, 'schema session phase bindings schedule steps cleanup closure non_claims', 'observation')
    _text(doc['session'], 'session')
    _bindings(doc['bindings']); _schedule(doc['schedule'])
    if doc['non_claims'] != NON_CLAIMS:
        _fail('non-claims differ')
    if type(doc['steps']) is not list or len(doc['steps']) != len(doc['schedule']):
        _fail('incomplete step list')
    invocations = set()
    for step, declaration in zip(doc['steps'], doc['schedule']):
        _step(step, declaration, invocations)
    closure = _object(doc['closure'], 'reason stop_step prefix_sha256 admission_sha256 consumption_sha256', 'closure')
    for key in ('prefix_sha256', 'admission_sha256', 'consumption_sha256'):
        if closure[key] is not None:
            _digest(closure[key])
    if kind == 'prefix' and any(closure[key] is not None for key in ('prefix_sha256', 'admission_sha256', 'consumption_sha256')):
        _fail('prefix cannot bind future artifacts')
    cleanup = _object(doc['cleanup'], 'restored isolated_tree_removed evidence_sha256', 'cleanup')
    _digest(cleanup['evidence_sha256'])
    if any(type(cleanup[k]) is not bool for k in ('restored', 'isolated_tree_removed')):
        _fail('invalid cleanup state')
    phase = doc['phase']
    if phase != 'stopped' and not (cleanup['restored'] and cleanup['isolated_tree_removed']):
        _fail('cleanup failure cannot authorize continuation')
    if phase == 'awaiting_admission' and kind == 'prefix':
        for declaration, step in zip(doc['schedule'], doc['steps']):
            if step['state'] != ('pending' if declaration['kind'] == 'ordinary' else 'complete'):
                _fail('invalid awaiting prefix')
        if closure['reason'] is not None or closure['stop_step'] is not None:
            _fail('awaiting prefix claims stop')
    elif phase == 'stopped':
        _reason(closure['reason'])
        seen = False
        record = None
        for step in doc['steps']:
            if step['step_id'] == closure['stop_step']:
                if step['state'] != 'stopped':
                    _fail('stop step is not stopped')
                seen = True
                record = step['failure'] or step['preflight']
                if record['reason'] != closure['reason']:
                    _fail('stop reason mismatch')
            elif step['state'] != ('not_run' if seen else 'complete'):
                _fail('invalid stopped suffix')
            if seen:
                for slot in step['slots']:
                    if slot['state'] == 'not_run' and (slot['reason'] != record['reason'] or slot['evidence_sha256'] != record['evidence_sha256']):
                        _fail('not_run slot differs from stop evidence')
        if not seen:
            _fail('missing stop step')
    elif kind == 'final' and phase in ('complete', 'refused'):
        _digest(closure['prefix_sha256']); _digest(closure['admission_sha256'])
        if closure['stop_step'] is not None:
            _fail('non-stopped final names stopped step')
        for declaration, step in zip(doc['schedule'], doc['steps']):
            expected = 'not_run' if phase == 'refused' and declaration['kind'] == 'ordinary' else 'complete'
            if step['state'] != expected:
                _fail('incomplete final')
            if expected == 'not_run':
                for slot in step['slots']:
                    if slot['reason'] != closure['reason'] or slot['evidence_sha256'] != closure['admission_sha256']:
                        _fail('refused slot differs from admission')
        if phase == 'complete':
            _digest(closure['consumption_sha256'])
            if closure['reason'] is not None:
                _fail('complete final has stop reason')
        else:
            if closure['reason'] not in ADMISSION_REASONS or closure['consumption_sha256'] is not None:
                _fail('invalid refusal closure')
    else:
        _fail('invalid phase')
    if kind == 'final':
        _digest(closure['prefix_sha256'])
        if any(slot['state'] == 'pending' for step in doc['steps'] for slot in step['slots']):
            _fail('final has pending slots')


def _admission(doc):
    _object(doc, 'schema session prefix_sha256 context_sha256 decision_sha256 policy_identity '
            'interpreter_identity next_step decision reasons nonce decision_binding_sha256', 'admission')
    for key in ('session', 'next_step', 'nonce'):
        _text(doc[key], key)
    for key in ('prefix_sha256', 'context_sha256', 'decision_sha256', 'policy_identity',
                'interpreter_identity', 'decision_binding_sha256'):
        _digest(doc[key])
    _strings(doc['reasons'], 'reasons')
    if not set(doc['reasons']) <= ADMISSION_REASONS:
        _fail('unknown admission reason')
    if doc['decision'] not in ('allow', 'refuse') or bool(doc['reasons']) != (doc['decision'] == 'refuse'):
        _fail('invalid decision/reason')
    unsigned = dict(doc); unsigned.pop('decision_binding_sha256')
    if sha256(canonical_bytes(unsigned)) != doc['decision_binding_sha256']:
        _fail('admission binding digest mismatch')


def encode_observation(doc):
    """Validate a closed artifact and return canonical bytes."""
    raw = canonical_bytes(doc)
    if type(doc) is not dict or doc.get('schema') not in SCHEMAS.values():
        _fail('unknown observation schema')
    kind = next(key for key, value in SCHEMAS.items() if value == doc['schema'])
    if kind == 'admission':
        _admission(doc)
    else:
        _observation(doc, kind)
    return raw


def load_observation(raw, *, kind):
    """Accept exact canonical bytes, not an equivalent reserialization."""
    if kind not in SCHEMAS or type(raw) is not bytes or len(raw) > MAX_BYTES:
        _fail('invalid kind or bounded bytes')
    try:
        doc = json.loads(raw.decode('utf-8'), object_pairs_hook=_pairs,
                         parse_constant=lambda _: _fail('non-finite number'))
    except (UnicodeError, RecursionError) as exc:
        raise ValueError('invalid JSON encoding/depth') from exc
    if type(doc) is not dict or doc.get('schema') != SCHEMAS[kind]:
        _fail('schema differs from expected kind')
    if encode_observation(doc) != raw:
        _fail('noncanonical observation bytes')
    return doc


def validate_admission(prefix, admission, *, context_raw, decision_raw):
    """Bind opaque operator bytes; never authenticate or interpret private policy."""
    prefix_raw = encode_observation(prefix)
    encode_observation(admission)
    if prefix['schema'] != PREFIX_SCHEMA or prefix['phase'] != 'awaiting_admission':
        _fail('prefix is not awaiting admission')
    expected = {'session': prefix['session'], 'prefix_sha256': sha256(prefix_raw),
                'context_sha256': sha256(context_raw), 'decision_sha256': sha256(decision_raw),
                'policy_identity': prefix['bindings']['policy_identity'],
                'interpreter_identity': prefix['bindings']['interpreter_identity'],
                'next_step': next(row['step_id'] for row in prefix['schedule'] if row['kind'] == 'ordinary')}
    if expected['context_sha256'] != prefix['bindings']['context_sha256']:
        _fail('context differs from prefix')
    if any(admission[key] != value for key, value in expected.items()):
        _fail('admission differs from predecessor bindings')


def prepare_closure(prefix_raw, *, admission_raw=None, context_raw=None, decision_raw=None):
    """Prepare a stopped/refused final without executing or publishing anything."""
    doc = load_observation(prefix_raw, kind='prefix')
    admission_digest = None
    if doc['phase'] == 'stopped':
        if any(value is not None for value in (admission_raw, context_raw, decision_raw)):
            _fail('stopped prefix takes no admission')
    else:
        if any(type(value) is not bytes for value in (admission_raw, context_raw, decision_raw)):
            _fail('awaiting closure requires admission, context and decision bytes')
        admission = load_observation(admission_raw, kind='admission')
        validate_admission(doc, admission, context_raw=context_raw, decision_raw=decision_raw)
        if admission['decision'] != 'refuse':
            _fail('allow admission cannot close without execution')
        admission_digest = sha256(admission_raw)
        reason = admission['reasons'][0]
        doc['phase'] = 'refused'; doc['closure']['reason'] = reason
        for row, step in zip(doc['schedule'], doc['steps']):
            if row['kind'] != 'ordinary':
                continue
            step.update(state='not_run', application='not_run', restored=True)
            for slot in step['slots']:
                slot.update(state='not_run', reason=reason, evidence_sha256=admission_digest)
    doc['schema'] = FINAL_SCHEMA
    doc['closure'].update(prefix_sha256=sha256(prefix_raw), admission_sha256=admission_digest)
    return encode_observation(doc)
