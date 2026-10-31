import signal
signal.alarm(8)

# front.py: the Python child of the refund C3 facade (CLAUDE-ADAPTER-DESIGN-v6 sections 4-9, with the addenda).
# The two statements above must stay first: they bound this process's lifetime by the kernel (design v6 C-3).
# Original semantic output rules remain below. Instrumentation failure also stops output; its sidecar is external to T.
# The steps run in the order of design v6 section 6: P0, P1a-P1e, P6, P2, P3, P4a-P4d, P5a-P5b, P7, R1-R5, Q, K.
import base64
import contextlib
import hashlib
import importlib.util
import io
import json
import os
import re
import stat
import sys

ADAPTER_VERSION = '0.1.0'
CLAIM_ID = 'c3.retained_report.definite_support.projected'
OUTPUT_SCHEMA = 'refund.c3-adapter-output.v0'
REQUEST_SCHEMA = 'refund.c3-front-request.v0'
EVIDENCE_SCHEMA = 'refund.c3-adapter-evidence.v0'
CONFIG_SCHEMA = 'refund.c3-adapter-config.v0'
PROFILE = 'refund.synthetic-c3.v0'
AXIS = 'definite_support'
COMMIT_PROFILE = 'refund.c3-selected-report.v0'
SELECTION_RULE = 'c3.report-id-unique.event-seq-equal.v0'
ENVELOPE_LIMIT = 262144
PACKET_LIMIT = 65536
NATIVE_LIMIT = 131072
MAX_DEPTH = 32
EX_USAGE = 64
EXIT_P0 = 4
ID = re.compile(r'[A-Za-z0-9_.:-]{1,128}')
HEX64 = re.compile(r'[0-9a-f]{64}')
HEX40 = re.compile(r'[0-9a-f]{40}')
B64 = re.compile(r'[A-Za-z0-9+/]*={0,2}')
ENVELOPE_KEYS = {'schema', 'admission_operation_id', 'profile', 'axis', 'selector', 'packet', 'native_output'}
SELECTOR_KEYS = ('evidence_operation_id', 'case_id', 'report_id', 'report_seq')
SOURCES = ('retained-observation', 'controlled-seam')
RESULTS = ('ESTABLISHED', 'CONTRADICTED', 'NOT_ESTABLISHED', None)
RECORD_KEYS = {'kind', 'profile', 'row', 'subject', 'axis', 'native', 'context', 'projected', 'binding'}


class Refusal(Exception):
    def __init__(self, layer, code):
        super().__init__(code)
        self.layer = layer
        self.code = code


def sha256(data):
    return hashlib.sha256(data).hexdigest()


def strict_json(raw):
    """UTF-8, no BOM, no duplicate keys, no NaN or Infinity, container depth at most 32."""
    def pairs(items):
        keys = [k for k, _ in items]
        if len(set(keys)) != len(keys):
            raise ValueError('duplicate key')
        return dict(items)

    def constant(name):
        raise ValueError(name)

    text = raw.decode('utf-8')
    if text.startswith('﻿'):
        raise ValueError('bom')
    doc = json.loads(text, object_pairs_hook=pairs, parse_constant=constant)
    stack = [(doc, 0)]
    while stack:
        value, parent = stack.pop()
        if isinstance(value, (dict, list)):
            depth = parent + 1
            if depth > MAX_DEPTH:
                raise ValueError('depth')
            stack.extend((child, depth) for child in (value.values() if isinstance(value, dict) else value))
    return doc


def read_file(path):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    with os.fdopen(fd, 'rb') as f:
        return f.read()


def write_excl(path, data):
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, 'wb') as f:
        f.write(data)


def is_id(v):
    return type(v) is str and ID.fullmatch(v) is not None


def is_seq(v):
    return type(v) is int and 1 <= v <= 1000


def is_hex64(v):
    return type(v) is str and HEX64.fullmatch(v) is not None


def utf16_len(s):
    return len(s.encode('utf-16-le', 'surrogatepass')) // 2


def valid_config(c):
    """Design v6 section 3, closed at every level."""
    if type(c) is not dict or set(c) != {'schema', 'python', 'authorized_component_tuples', 'native_output_allowlist'}:
        return False
    if c['schema'] != CONFIG_SCHEMA:
        return False
    p = c['python']
    if type(p) is not dict or set(p) != {'path', 'version_observation'}:
        return False
    path = p['path']
    if type(path) is not str or not 1 <= utf16_len(path) <= 4096 or not path.startswith('/') or '\x00' in path:
        return False
    if type(p['version_observation']) is not str:
        return False
    tuples = c['authorized_component_tuples']
    if type(tuples) is not list or not tuples:
        return False
    for t in tuples:
        if type(t) is not dict or set(t) != {'label', 'front_py', 'replay_native_py', 'projector_py'}:
            return False
        if type(t['label']) is not str or not all(is_hex64(t[k]) for k in ('front_py', 'replay_native_py', 'projector_py')):
            return False
    allow = c['native_output_allowlist']
    if type(allow) is not list or not allow:
        return False
    seen = set()
    for a in allow:
        if type(a) is not dict or set(a) != {'sha256', 'source', 'label', 'provenance'}:
            return False
        if not is_hex64(a['sha256']) or a['sha256'] in seen or type(a['label']) is not str:
            return False
        seen.add(a['sha256'])
        if a['source'] == 'retained-observation':
            pv = a['provenance']
            if type(pv) is not dict or set(pv) != {'lab_commit', 'receipt_path', 'blob_path'}:
                return False
            if type(pv['lab_commit']) is not str or HEX40.fullmatch(pv['lab_commit']) is None:
                return False
            if type(pv['receipt_path']) is not str or type(pv['blob_path']) is not str:
                return False
        elif a['source'] == 'controlled-seam':
            if a['provenance'] is not None:
                return False
        else:
            return False
    return True


def valid_request(req):
    return (type(req) is dict and set(req) == {'schema', 'admission_operation_id', 'component_dir', 'config', 'integration'}
            and req['schema'] == REQUEST_SCHEMA and type(req['admission_operation_id']) is str
            and req['admission_operation_id'] != '' and type(req['component_dir']) is str
            and valid_config(req['config']) and type(req['integration']) is dict)


def envelope_fields_valid(env):
    """P1e: every remaining closedness, type and format rule of design v6 section 4."""
    if type(env['admission_operation_id']) is not str or env['admission_operation_id'] == '':
        return False
    if type(env['profile']) is not str or type(env['axis']) is not str:
        return False
    sel = env['selector']
    if set(sel) != set(SELECTOR_KEYS):
        return False
    if not all(is_id(sel[k]) for k in ('evidence_operation_id', 'case_id', 'report_id')) or not is_seq(sel['report_seq']):
        return False
    p = env['packet']
    if type(p) is not dict or set(p) != {'sha256', 'bytes_b64'} or not is_hex64(p['sha256']):
        return False
    if type(p['bytes_b64']) is not str:
        return False
    n = env['native_output']
    if type(n) is not dict or set(n) != {'source', 'sha256', 'bytes_b64'} or n['source'] not in SOURCES:
        return False
    return is_hex64(n['sha256']) and type(n['bytes_b64']) is str


def decode_b64(text):
    if B64.fullmatch(text) is None:
        raise Refusal('front', 'evidence:base64_invalid')
    try:
        data = base64.b64decode(text, validate=True)
    except ValueError:
        raise Refusal('front', 'evidence:base64_invalid') from None
    if not data:
        raise Refusal('front', 'evidence:base64_invalid')
    return data


def load_projector(component_dir):
    spec = importlib.util.spec_from_file_location('projector', os.path.join(component_dir, 'py', 'projector.py'))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def record_digest(record):
    """Strict: a value JSON cannot carry exactly (NaN, a set, ...) raises instead of being written."""
    return sha256(json.dumps(record, sort_keys=True, separators=(',', ':'), ensure_ascii=True,
                             allow_nan=False).encode('ascii'))


def postcondition_q(record, env, verified):
    """Q1a, Q1b, Q2, Q3 of design v6 section 7. Returns a failure code or None."""
    subject = record.get('subject') if type(record) is dict else None
    if type(subject) is not dict or set(subject) != {'case_id', 'operation_id', 'report_id', 'report_seq'}:
        return 'adapter:selected_identity_invalid'
    if not all(is_id(subject[k]) for k in ('case_id', 'operation_id', 'report_id')) or not is_seq(subject['report_seq']):
        return 'adapter:selected_identity_invalid'
    if set(record) != RECORD_KEYS or record['kind'] != 'claim' or record['profile'] != env['profile']:
        return 'adapter:projector_record_invalid'
    if record['axis'] != env['axis'] or type(record['row']) is not str:
        return 'adapter:projector_record_invalid'
    native = record['native']
    if type(native) is not dict or set(native) != {'applicable', 'result', 'reason'}:
        return 'adapter:projector_record_invalid'
    if type(native['applicable']) is not bool or native['result'] not in RESULTS:
        return 'adapter:projector_record_invalid'
    if type(native['reason']) is not str or native['reason'] == '':
        return 'adapter:projector_record_invalid'
    context = record['context']
    if type(context) is not dict or set(context) != {'delivery_coverage', 'source_class', 'conflict'}:
        return 'adapter:projector_record_invalid'
    if context['delivery_coverage'] not in ('complete', 'incomplete') or type(context['source_class']) is not str:
        return 'adapter:projector_record_invalid'
    if type(context['conflict']) is not bool or record['projected'] not in ('established', 'not_established'):
        return 'adapter:projector_record_invalid'
    binding = record['binding']
    if type(binding) is not dict or set(binding) != {'packet_sha256', 'report_sha256', 'mapping_revision'}:
        return 'adapter:projector_record_invalid'
    if not all(is_hex64(binding[k]) for k in ('packet_sha256', 'report_sha256', 'mapping_revision')):
        return 'adapter:projector_record_invalid'
    sel = env['selector']
    if (subject['case_id'], subject['operation_id'], subject['report_id'], subject['report_seq']) != (
            sel['case_id'], sel['evidence_operation_id'], sel['report_id'], sel['report_seq']):
        return 'adapter:selected_identity_mismatch'
    if binding['packet_sha256'] != verified['packet'] or binding['report_sha256'] != verified['native']:
        return 'adapter:selected_binding_mismatch'
    return None


def enc_s(v):
    if type(v) is not str or any(not 0x20 <= ord(ch) <= 0x7e for ch in v):
        raise ValueError('s')
    return 's:' + v


def enc_i(v):
    if type(v) is not int or not 1 <= v <= 1000:
        raise ValueError('i')
    return 'i:%d' % v


def enc_b(v):
    if type(v) is not bool:
        raise ValueError('b')
    return 'b:true' if v else 'b:false'


def commitment(record, env, verified):
    """K, design v6 section 9: the encoder's own type check, independent of Q."""
    try:
        subject, native, context, binding = record['subject'], record['native'], record['context'], record['binding']
        fields = [
            ('case_id', enc_s(subject['case_id'])),
            ('packet_sha256', enc_s(verified['packet'])),
            ('native_output_sha256', enc_s(verified['native'])),
            ('evidence_operation_id', enc_s(subject['operation_id'])),
            ('report_id', enc_s(subject['report_id'])),
            ('report_seq', enc_i(subject['report_seq'])),
            ('axis', enc_s(env['axis'])),
            ('native_applicable', enc_b(native['applicable'])),
            ('native_result', 'n:' if native['result'] is None else enc_s(native['result'])),
            ('native_reason', enc_s(native['reason'])),
            ('context_delivery_coverage', enc_s(context['delivery_coverage'])),
            ('context_source_class', enc_s(context['source_class'])),
            ('context_conflict', enc_b(context['conflict'])),
            ('projected', enc_s(record['projected'])),
            ('selection_rule', enc_s(SELECTION_RULE)),
            ('mapping_revision', enc_s(binding['mapping_revision'])),
        ]
    except (KeyError, TypeError, ValueError):
        raise Refusal('commitment', 'adapter:commitment_value_invalid') from None
    body = COMMIT_PROFILE + '\n' + ''.join('%d\n%s%d\n%s' % (len(n), n, len(v), v) for n, v in fields)
    return [[n, v] for n, v in fields], sha256(body.encode('ascii'))


def steps(T, req, ev_raw, out):
    """P1a-K. Fills the front-origin fields of out as each gate passes; raises Refusal on the first failure."""
    if len(ev_raw) > ENVELOPE_LIMIT:
        raise Refusal('front', 'evidence:too_large')
    try:
        env = strict_json(ev_raw)
    except (ValueError, RecursionError):
        raise Refusal('front', 'evidence:json_invalid') from None
    if type(env) is not dict or set(env) != ENVELOPE_KEYS or env['schema'] != EVIDENCE_SCHEMA:
        raise Refusal('front', 'evidence:schema_invalid')
    sel = env['selector']
    if type(sel) is not dict or any(k not in sel for k in SELECTOR_KEYS):
        raise Refusal('front', 'evidence:selector_missing')
    if not envelope_fields_valid(env):
        raise Refusal('front', 'evidence:field_invalid')
    out['selector'] = {k: sel[k] for k in SELECTOR_KEYS}
    if env['profile'] != PROFILE:
        raise Refusal('front', 'unsupported:profile')
    if env['axis'] != AXIS:
        raise Refusal('front', 'unsupported:axis')
    if env['admission_operation_id'] != req['admission_operation_id']:
        raise Refusal('front', 'evidence:admission_operation_mismatch')
    if sel['evidence_operation_id'] == req['admission_operation_id']:
        raise Refusal('front', 'evidence:evidence_operation_is_admission')
    packet = decode_b64(env['packet']['bytes_b64'])
    native = decode_b64(env['native_output']['bytes_b64'])
    if len(packet) > PACKET_LIMIT or len(native) > NATIVE_LIMIT:
        raise Refusal('front', 'evidence:too_large')
    verified = {'packet': sha256(packet), 'native': sha256(native)}
    if verified['packet'] != env['packet']['sha256']:
        raise Refusal('front', 'evidence:packet_digest_mismatch')
    if verified['native'] != env['native_output']['sha256']:
        raise Refusal('front', 'evidence:native_output_digest_mismatch')
    entries = [a for a in req['config']['native_output_allowlist'] if a['sha256'] == verified['native']]
    if not entries:
        raise Refusal('front', 'evidence:native_output_not_allowlisted')
    if entries[0]['source'] != env['native_output']['source']:
        raise Refusal('front', 'evidence:native_source_mismatch')
    out['inputs'] = {'native_source': env['native_output']['source'], 'packet_sha256': verified['packet'],
                     'native_output_sha256': verified['native']}
    try:
        write_excl(os.path.join(T, 'packet.json'), packet)
        write_excl(os.path.join(T, 'native.out'), native)
    except OSError:
        raise Refusal('front', 'adapter:workdir_failed') from None

    component_dir = req['component_dir']
    python = req['config']['python']['path']
    argv = ['wrap', '--profile=' + PROFILE, '--packet=' + os.path.join(T, 'packet.json'),
            '--packet-sha256=' + env['packet']['sha256'], '--operation=' + sel['evidence_operation_id'],
            '--report=' + sel['report_id'], '--axis=' + AXIS, '--timeout=2',
            '--', python, '-I', '-P', '-B', os.path.join(component_dir, 'py', 'replay_native.py'),
            os.path.join(T, 'native.out'), env['native_output']['sha256']]
    buf_out, buf_err = io.StringIO(), io.StringIO()
    try:
        with contextlib.redirect_stdout(buf_out), contextlib.redirect_stderr(buf_err):
            ACTIVE_BRIDGE.before_replay(argv[argv.index('--') + 1:], verified['native'])
            projector = load_projector(component_dir)
            code, record = projector.run(argv)
    except Exception as exc:
        raise Refusal('projector', 'crash:projector:' + type(exc).__name__) from None
    # A record that run() returned is kept verbatim with its digest on every later outcome (design v6 section 8),
    # unless it cannot be serialized exactly; then it is not kept and is never a success. Precedence is unchanged:
    # R2, then R3, then record-invalid.
    if type(record) is dict:
        try:
            out['projector_record_sha256'] = record_digest(record)
            out['projector_record'] = record
        except (TypeError, ValueError, RecursionError):
            out['projector_record_sha256'] = None
    if code == EX_USAGE:
        raise Refusal('projector', 'adapter:projector_usage')
    if buf_out.getvalue() or buf_err.getvalue():
        raise Refusal('projector', 'adapter:projector_wrote_output')
    if code != 0 or type(record) is not dict or out['projector_record'] is None:
        raise Refusal('postcondition', 'adapter:projector_record_invalid')
    kind = record.get('kind')
    if kind == 'execution_record':
        if type(record.get('state')) is not str:
            raise Refusal('postcondition', 'adapter:projector_record_invalid')
        raise Refusal('native_output', 'native_output:' + record['state'])
    if kind == 'no_claim':
        if type(record.get('reason')) is not str:
            raise Refusal('postcondition', 'adapter:projector_record_invalid')
        raise Refusal('projector', 'selection:' + record['reason'])
    if kind != 'claim':
        raise Refusal('postcondition', 'adapter:projector_record_invalid')

    q_failure = None
    if q_failure is not None:
        raise Refusal('postcondition', q_failure)

    fields, digest = commitment(record, env, verified)
    subject, native = record['subject'], record['native']
    semantic = {
        'selected': {'case_id': subject['case_id'], 'evidence_operation_id': subject['operation_id'],
                     'report_id': subject['report_id'], 'report_seq': subject['report_seq']},
        'native': native, 'context': record['context'], 'projected': record['projected'],
        'selected_report_commitment': {'profile': COMMIT_PROFILE, 'fields': fields, 'sha256': digest},
    }
    result = 'null' if native['result'] is None else native['result']
    claim = {'claim': CLAIM_ID, 'status': record['projected'], 'reason': 'native:%s:%s' % (result, native['reason'])}
    return semantic, claim


def evaluate(T, req, req_raw, ev_raw):
    out = {'schema': OUTPUT_SCHEMA, 'adapter_version': ADAPTER_VERSION,
           'admission_operation_id': req['admission_operation_id'],
           'request_evidence_sha256': sha256(ev_raw), 'front_request_sha256': sha256(req_raw),
           'inputs': None, 'selector': None, 'projector_record': None, 'projector_record_sha256': None,
           'semantic': None}
    try:
        semantic, claim = steps(T, req, ev_raw, out)
    except Refusal as refusal:
        status = 'unsupported' if refusal.code.startswith('unsupported:') else 'failed'
        out['failure'] = {'layer': refusal.layer, 'code': refusal.code, 'detail': None}
        out['claim'] = {'claim': CLAIM_ID, 'status': status, 'reason': refusal.code}
        return out
    out['semantic'] = semantic
    out['failure'] = None
    out['claim'] = claim
    return out


def main(argv):
    if len(argv) != 2:
        return EXIT_P0
    T = argv[1]
    try:
        req_raw = read_file(os.path.join(T, 'request.json'))
        ev_raw = read_file(os.path.join(T, 'evidence.bin'))
        req = strict_json(req_raw)
    except (OSError, ValueError, RecursionError):
        return EXIT_P0
    if not valid_request(req):
        return EXIT_P0
    if os.path.dirname(os.path.dirname(os.path.abspath(__file__))) != req['component_dir']:
        return EXIT_P0
    global ACTIVE_BRIDGE
    binding = req['integration']
    # The explicit trusted request pins a loader at the exact instrumented source root.
    bridge_path = os.path.abspath(os.path.join(os.path.dirname(__file__), '..', '..', 'integration_bridge.py'))
    if binding.get('bridge_path') != bridge_path:
        return EXIT_P0
    loader_fd = os.open(bridge_path, os.O_RDONLY | os.O_NOFOLLOW)
    loader_primary = None
    try:
        loader_stat = os.fstat(loader_fd)
        if not stat.S_ISREG(loader_stat.st_mode) or loader_stat.st_size > 49152:
            return EXIT_P0
        raw = os.read(loader_fd, 49153)
        if len(raw) != loader_stat.st_size or len(raw) > 49152 or sha256(raw) != binding.get('bridge_sha256'):
            return EXIT_P0
    except BaseException as exc:
        loader_primary = exc
        raise
    finally:
        try:
            os.close(loader_fd)
        except BaseException:
            if loader_primary is None:
                raise
    spec = importlib.util.spec_from_file_location('c3_integration_bridge', bridge_path)
    bridge_module = importlib.util.module_from_spec(spec)
    # Execute the checked byte buffer, not a second read through the path loader.
    exec(compile(raw, bridge_path, 'exec'), bridge_module.__dict__)
    ACTIVE_BRIDGE = bridge_module.Bridge(binding, T, req['component_dir'])
    first = None
    try:
        out = evaluate(T, req, req_raw, ev_raw)
    except BaseException as exc:
        first = exc
        raise
    finally:
        try:
            ACTIVE_BRIDGE.finish(first)
        except BaseException:
            if first is None:
                raise
    sys.stdout.write(json.dumps(out, sort_keys=True, separators=(',', ':'), ensure_ascii=True, allow_nan=False) + '\n')
    sys.stdout.flush()
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
