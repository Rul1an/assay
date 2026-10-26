"""Offline, bounded observation check. No oracle, network, or authenticity claim.

Run against a retained capture directory. `closed` means the producer recorded an
ending and the retained decision bytes match its accounting, not tool success.
"""
import hashlib
import json
import math
from pathlib import Path
import sys

# Reuse the capture helper's regular-file, no-symlink bounded read and JSON hooks.
from record_hook import _read, _pairs, _nonfinite

MAX_LOG = 8 * 1024 * 1024
MAX_EVENTS = 10000


def require(value, reason):
    if not value:
        raise ValueError(reason)


def parse(raw):
    depth = 0; quoted = False; escaped = False
    for byte in raw:
        if quoted:
            if escaped: escaped = False
            elif byte == 92: escaped = True
            elif byte == 34: quoted = False
        elif byte == 34: quoted = True
        elif byte in (91, 123):
            depth += 1
            require(depth <= 32, 'json_depth')
        elif byte in (93, 125): depth -= 1
    def finite(token):
        value = float(token)
        require(math.isfinite(value), 'nonfinite')
        return value
    value = json.loads(raw, object_pairs_hook=_pairs, parse_constant=_nonfinite, parse_float=finite)
    require(type(value) is dict, 'object_required')
    return value


def verify(root):
    """Read originals without rewriting. Refusal reasons never echo input bytes."""
    try:
        root = Path(root)
        stop = parse(_read(root/'session-stop.json', 65536))
        context = parse(_read(root/'session-context.json', 65536))
        raw = _read(root/'decisions.ndjson', MAX_LOG)
        require(stop.get('specversion') == '1.0' and stop.get('type') == 'assay.mcp.session.stop.v1', 'stop_type')
        require(context.get('type') == 'assay.mcp.session.context.v1', 'context_type')
        data = stop['data']; ctx = context['data']
        require(type(data) is dict and type(ctx) is dict, 'data_shape')
        cid = context['id']
        require(type(cid) is str and cid.startswith('evt_context_') and len(cid) > 12, 'context_id')
        require(data.get('session_context_id') == cid and stop.get('id') == 'evt_stop_'+cid[len('evt_context_'):], 'context_link')
        require(type(stop.get('source')) is str and stop['source'] == context.get('source'), 'source_link')
        require(type(data.get('server_id')) is str and data['server_id'] == ctx.get('server_id'), 'server_link')
        require(data.get('assertion_basis') == 'producer_observation', 'assertion_basis')
        written = data['decision_events_written']; dropped = data['decision_events_dropped']
        require(type(written) is int and 0 <= written <= MAX_EVENTS, 'written_count')
        require(type(dropped) is int and 0 <= dropped <= 2**64-1, 'dropped_count')
        require(type(data.get('server_pid')) is int and 0 < data['server_pid'] <= 2**32-1, 'pid')
        require(type(data.get('assay_kill_requested')) is bool, 'kill_shape')
        end = data.get('session_end')
        require(end in ('client_closed', 'proxy_error'), 'session_end')
        termination = data.get('termination'); code = data.get('exit_code'); signal = data.get('signal')
        core = data.get('core_dumped')
        require(core is None or type(core) is bool, 'core_shape')
        require((termination == 'exited' and type(code) is int and signal is None and core in (False, None))
                or (termination == 'signaled' and code is None and type(signal) is int and signal > 0)
                or (termination == 'unknown' and code is None and signal is None), 'termination_shape')
        state = data.get('decision_log')
        require(state in ('complete', 'incomplete'), 'unretained_log')
        lines = raw.splitlines(keepends=True)
        require(len(lines) <= MAX_EVENTS+1 and len(lines) >= written, 'line_count')
        prefix = lines[:written]
        require(all(line.endswith(b'\n') for line in prefix), 'partial_prefix')
        ids = set()
        for line in prefix:
            event = parse(line)
            require(event.get('type') == 'assay.tool.decision' and type(event.get('id')) is str, 'decision_shape')
            require(event['id'] not in ids, 'duplicate_decision')
            ids.add(event['id'])
        require(hashlib.sha256(b''.join(prefix)).hexdigest() == data.get('decision_prefix_sha256'), 'prefix_digest')
        if state == 'complete':
            require(dropped == 0 and end == 'client_closed' and len(lines) == written, 'false_complete')
        else:
            # A failed write may retain a partial tail, but never another complete event.
            require(len(lines) <= written+1, 'surplus_tail')
        observed = 'unknown' if termination == 'unknown' else ('closed' if state == 'complete' else 'incomplete')
        return {'status': observed, 'termination': termination, 'exit_code': code, 'signal': signal,
                'decision_events_written': written, 'decision_events_dropped': dropped,
                'claim': 'producer_observed_stop_and_retained_decision_prefix_only'}
    except (OSError, ValueError, KeyError, TypeError, RecursionError):
        return {'status': 'unknown', 'reason': 'missing_or_invalid_stop_evidence'}


if __name__ == '__main__':
    result = verify(Path(sys.argv[1])) if len(sys.argv) == 2 else {'status':'unknown','reason':'capture_directory_required'}
    print(json.dumps(result, sort_keys=True))
    raise SystemExit(0 if result['status'] == 'closed' else 1)
