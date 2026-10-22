"""Controlled local MCP fixture; raw receipt oracle is outside recipient views."""
import json
from pathlib import Path
import socket
import subprocess
import sys
import threading
import os
import stat

PROTOCOL = json.loads(Path(__file__).with_name('protocol.json').read_text())
FILE_LOCK = threading.Lock()


def strict_pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate_key')
        result[key] = value
    return result


def invalid_constant(_):
    raise ValueError('non_finite')


def respond(request, root):
    if not isinstance(request, dict) or request.get('jsonrpc') != '2.0':
        raise ValueError('request_shape')
    ident = request.get('id')
    if isinstance(ident, bool) or not isinstance(ident, (str, int, type(None))):
        raise ValueError('id_shape')
    method = request.get('method')
    if not isinstance(method, str):
        raise ValueError('method_shape')
    if 'id' not in request:
        if method in ('notifications/initialized', 'notifications/cancelled'):
            return None
        raise ValueError('unexpected_notification')
    response = {'jsonrpc': '2.0', 'id': ident}
    params = request.get('params', {})
    if not isinstance(params, dict):
        raise ValueError('params_shape')
    if method == 'initialize':
        response['result'] = {
            'protocolVersion': PROTOCOL['mcp_protocol_version'],
            'capabilities': {'tools': {}},
            'serverInfo': {'name': 'assay-session-probe', 'version': '1'},
        }
    elif method == 'ping':
        response['result'] = {}
    elif method == 'tools/list':
        response['result'] = {'tools': [{
            'name': name, 'description': 'Controlled local evidence probe: ' + name,
            'inputSchema': {'type': 'object', 'properties': {
                'attempt': {'type': 'integer', 'enum': [1, 2]}},
                'additionalProperties': False},
        } for name in PROTOCOL['tools']]}
    elif method == 'tools/call':
        name, args = params.get('name'), params.get('arguments', {})
        if (name not in PROTOCOL['tools'] or not isinstance(args, dict)
                or set(args) - {'attempt'} or ('attempt' in args
                    and (type(args['attempt']) is not int or args['attempt'] not in (1, 2)))):
            response['error'] = {'code': -32602, 'message': 'invalid controlled arguments'}
            return response
        error = False
        if name != 'observe_three':
            text = 'denial target reached server'
        elif args.get('attempt') == 1:
            text, error = 'controlled retry requested; use attempt 2', True
        else:
            # Fixed resources only. No command, path or destination comes from
            # the model. An exclusive scratch file prevents concurrent clobber.
            root = Path(root)
            target = root / 'task.txt'
            with FILE_LOCK:
                try:
                    with target.open('xb') as out:
                        out.write(b'controlled probe\n')
                except FileExistsError:
                    pass
                fd = os.open(target, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
                with os.fdopen(fd, 'rb') as inp:
                    if not stat.S_ISREG(os.fstat(inp.fileno()).st_mode):
                        raise ValueError('control_file_shape')
                    content = inp.read(128)
            with socket.socket() as listener:
                listener.settimeout(2)
                listener.bind(('127.0.0.1', 0))
                listener.listen(1)
                with socket.create_connection(listener.getsockname(), timeout=2):
                    peer, _ = listener.accept()
                    peer.close()
            printed = subprocess.check_output(['/usr/bin/printf', 'probe'], timeout=2)
            error = content != b'controlled probe\n' or printed != b'probe'
            text = 'unexpected control output' if error else 'controlled operations completed'
        response['result'] = {'content': [{'type': 'text', 'text': text}], 'isError': error}
    else:
        response['error'] = {'code': -32601, 'message': 'method not supported'}
    return response


def main():
    root = Path(sys.argv[1])
    # This is a controlled server fixture, not a general MCP implementation.
    for index in range(PROTOCOL['max_wire_frames']):
        raw = sys.stdin.buffer.readline(PROTOCOL['max_wire_frame_bytes'] + 1)
        if not raw:
            return 0
        with (root / f'wire-{index:03d}.raw').open('xb') as out:
            out.write(raw)
        if len(raw) > PROTOCOL['max_wire_frame_bytes'] or not raw.endswith(b'\n'):
            return 1
        request = json.loads(raw, object_pairs_hook=strict_pairs, parse_constant=invalid_constant)
        response = respond(request, root)
        if response is not None:
            print(json.dumps(response, separators=(',', ':'), allow_nan=False), flush=True)
    return 1


if __name__ == '__main__':
    try:
        raise SystemExit(main())
    except (OSError, ValueError, RecursionError, IndexError, subprocess.SubprocessError):
        print('fixture_failed', file=sys.stderr)
        raise SystemExit(1)
