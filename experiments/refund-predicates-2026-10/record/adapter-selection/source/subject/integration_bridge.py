"""Pinned E3 native/failure boundary, source-only. No default binding authority.
A prepared terminal cannot certify its own later persistence. On refusal, emit
one finite failure carrier to the already-owned front stderr; consumer validates
its actual captured bytes separately. This is not independent outer custody.
"""
import hashlib
import importlib.util
import json
import os
from pathlib import Path
import stat
import sys

CAP = 16384
MAX_WRITES = 4
FAILURE_PREFIX = b'C3-E3-FAILURE-V2 '
FAILURE_CAP = CAP + len(FAILURE_PREFIX) + 1
KEYS = {'schema', 'root', 'bridge_path', 'bridge_sha256', 'helper_path', 'helper_sha256', 'token', 'native'}
HEX = set('0123456789abcdef')

def digest(value):
    return type(value) is str and len(value) == 64 and set(value) <= HEX

def cause(reason, exc=None, phase=None):
    cls = type(exc).__name__ if exc is not None else None
    if cls is not None and (not cls.isidentifier() or len(cls) > 64):
        cls = 'Exception'
    err = getattr(exc, 'errno', None) if isinstance(exc, OSError) else None
    if type(err) is not int or not -4096 <= err <= 4096:
        err = None
    code = getattr(exc, 'code', None)
    if type(code) is not str or not 1 <= len(code) <= 64 or any(c not in 'ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789_-:' for c in code):
        code = None
    return {'reason': reason, 'phase': phase or reason, 'class': cls, 'code': code, 'errno': err}

def read_regular(path, cap=CAP):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    primary = None
    try:
        info = os.fstat(fd)
        if not stat.S_ISREG(info.st_mode) or info.st_size > cap:
            raise ValueError('regular file/cap required')
        data = os.read(fd, cap + 1)
        if len(data) != info.st_size or len(data) > cap:
            raise ValueError('incomplete read/cap')
        return data
    except BaseException as exc:
        primary = exc
        raise
    finally:
        try:
            os.close(fd)
        except BaseException:
            if primary is None:
                raise

def valid_binding(binding):
    return (type(binding) is dict and set(binding) == KEYS and binding['schema'] == 'c3.integration.request.v1'
            and all(type(binding[k]) is str and 0 < len(binding[k]) <= 1024 and os.path.isabs(binding[k])
                    for k in ('root', 'bridge_path', 'helper_path'))
            and all(digest(binding[k]) for k in ('bridge_sha256', 'helper_sha256', 'token'))
            and type(binding['native']) is dict)

def load_verified(path, sha, name):
    raw = read_regular(path, 49152)
    if not digest(sha) or hashlib.sha256(raw).hexdigest() != sha:
        raise ValueError('source pin')
    spec = importlib.util.spec_from_file_location(name, path)
    module = importlib.util.module_from_spec(spec)
    sys.modules[name] = module
    exec(compile(raw, path, 'exec'), module.__dict__)
    return module

def unavailable_provider():
    raise RuntimeError('native E3 resource provider unreleased')

class Bridge:
    def __init__(self, binding, T, component_dir, *, provider=None,
                 loader=load_verified, read=read_regular, io=os, gate_options=None,
                 failure_stream=None):
        if not valid_binding(binding):
            raise ValueError('integration binding missing')
        if not os.path.isabs(T) or not os.path.isabs(component_dir):
            raise ValueError('binding paths')
        root = os.path.normpath(binding['root'])
        if root == os.path.normpath(T) or root.startswith(os.path.normpath(T) + os.sep):
            raise ValueError('sidecar inside T')
        self.binding, self.T, self.component_dir = dict(binding), T, component_dir
        self.provider, self.loader, self.read, self.io = provider, loader, read, io
        self.gate_options = dict(gate_options or {})
        self.failure_stream = sys.stderr.buffer if failure_stream is None else failure_stream
        self.gate = None
        self.required = False
        self.first = None
        self.secondary = []
        self.finished = False
        self.write_attempts = 0
        self.persistence = None
        self.carrier = {'attempted': False, 'bytes': 0, 'sha256': None, 'write_return': None,
                        'flushed': False, 'complete': False}

    def remember(self, fact):
        if self.first is None:
            self.first = dict(fact)
        elif fact != self.first and fact not in self.secondary and len(self.secondary) < 4:
            self.secondary.append(dict(fact))

    def fail(self, reason, exc=None, phase=None):
        self.remember(cause(reason, exc, phase))

    def gate_failure(self, state=None):
        state = self.gate.snapshot() if state is None and self.gate else state
        if state and state.get('first_cause'):
            original = state['first_cause']
            # The source-pinned gate emits bounded canonical reason/phase/class/errno.
            self.remember(dict(original, code=None))
            for later in state.get('secondary', [])[:4]:
                self.remember(dict(later, code=None))
        return state

    def before_replay(self, command, native_sha):
        if self.required or self.finished or self.first:
            self.fail('registration_repeated_or_late')
            raise RuntimeError('E3 integration refused')
        self.required = True
        try:
            if (type(command) is not list or len(command) != 7 or not digest(native_sha)
                    or command[1:4] != ['-I', '-P', '-B']
                    or command[4] != os.path.join(self.component_dir, 'py', 'replay_native.py')
                    or command[5] != os.path.join(self.T, 'native.out') or command[6] != native_sha):
                raise ValueError('replay exact binding')
            cfg = self.binding['native']
            if type(cfg) is not dict or set(cfg) != {'provider','contract','release','binding'}:
                raise ValueError('native E3 binding')
            for key in ('provider','contract'):
                pin = cfg[key]
                if type(pin) is not dict or set(pin) != {'path','sha256'} or not digest(pin['sha256']) or not os.path.isabs(pin['path']):
                    raise ValueError('native source pin')
            contract = self.loader(cfg['contract']['path'], cfg['contract']['sha256'], 'native_contract')
            b = contract.bind(cfg['binding'], 'python-dylib', 524288)
            if b['mode'] != 'native' or b['provider_sha256'] != cfg['provider']['sha256'] or b['volume'] != self.binding['root']:
                raise ValueError('native E3 source binding')
            provider_module = self.loader(cfg['provider']['path'], cfg['provider']['sha256'], 'c3_e3_native_provider')
            release = cfg['release']
            if type(release) is not dict or any(release.get(k) != b['binary'][k] for k in ('path','bytes','sha256')):
                raise ValueError('native E3 binary binding')
            if os.path.realpath(command[0]) != os.path.realpath(sys.executable) or hashlib.sha256(self.read(command[0],16777216)).hexdigest()!=b['runtime_sha256']:
                raise ValueError('native E3 runtime binding')
            backend = provider_module.load_native(release)
            observer = provider_module.Provider(backend, volume=b['volume'], reserve_bytes=524288,
                max_latency_ns=int(b['max_latency_ns']),max_age_ns=int(b['max_age_ns']),
                max_pair_ns=int(b['max_pair_ns']),max_attempts=1)
            if self.provider is not None:
                raise ValueError('injected E3 provider not an actual native integration')
            # Load contract first: parent imports that exact canonical module.
            parent = self.loader(self.binding['helper_path'], self.binding['helper_sha256'], 'c3_e3_parent_admission')
            policy = parent.Policy('E3', command[0], tuple(command), None, None)
            self.gate = parent.Gate((policy,), observer.observe, self.binding['root'], reserve_bytes=524288,
                native_binding=b, clock=backend.now,policy_limit=1,attempt_limit=1,witness_limit=2, **self.gate_options)
            self.gate.install()
            self.gate.require_ready()
        except BaseException as exc:
            self.gate_failure()  # gate refusal actually precedes this caught wrapper
            self.fail('E3_setup', exc)
            raise

    def persist(self, raw):
        io = self.io
        directory = fd = None
        primary = None
        phase = 'terminal-directory-open'
        fact = {'created': False, 'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest(),
                'write_return': None, 'file_fsynced': False, 'file_close_ok': False,
                'directory_fsynced': False, 'directory_close_ok': False, 'complete': False, 'errors': []}
        self.persistence = fact
        def note(exc, stage):
            nonlocal primary
            record = cause(stage, exc)
            if len(fact['errors']) < 4:
                fact['errors'].append(record)
            self.remember(record)
            if primary is None:
                primary = exc
        try:
            if self.write_attempts >= MAX_WRITES or len(raw) > CAP:
                raise ValueError('terminal cap')
            self.write_attempts += 1
            directory = io.open(self.binding['root'], io.O_RDONLY | io.O_DIRECTORY | io.O_NOFOLLOW)
            phase = 'terminal-directory-type'
            if not stat.S_ISDIR(io.fstat(directory).st_mode):
                raise ValueError('sidecar directory')
            phase = 'terminal-file-open'
            fd = io.open('terminal.json', io.O_WRONLY | io.O_CREAT | io.O_EXCL | io.O_NOFOLLOW, 0o600, dir_fd=directory)
            fact['created'] = True
            phase = 'terminal-write'
            count = io.write(fd, raw)
            fact['write_return'] = count if type(count) is int and -1 <= count <= CAP else None
            if type(count) is not int or count != len(raw):
                raise ValueError('partial terminal')
            phase = 'terminal-file-fsync'
            io.fsync(fd)
            fact['file_fsynced'] = True
        except BaseException as exc:
            note(exc, phase)
        finally:
            if fd is not None:
                try:
                    io.close(fd)
                    fact['file_close_ok'] = True
                except BaseException as exc:
                    note(exc, 'terminal-file-close')
            if directory is not None:
                if primary is None:
                    try:
                        io.fsync(directory)
                        fact['directory_fsynced'] = True
                    except BaseException as exc:
                        note(exc, 'terminal-directory-fsync')
                try:
                    io.close(directory)
                    fact['directory_close_ok'] = True
                except BaseException as exc:
                    note(exc, 'terminal-directory-close')
        if primary is not None:
            raise primary
        fact['complete'] = True

    def emit_failure(self, state):
        if self.carrier['attempted']:
            return
        self.carrier['attempted'] = True
        try:
            doc = {'schema': 'c3.integration.e3-failure.v2', 'token': self.binding['token'], 'T': self.T,
                   'required': self.required, 'state': state, 'first_failure': self.first,
                   'secondary': list(self.secondary), 'persistence': self.persistence,
                   'scope': 'front-owned stderr bytes; no independent custody or EOF proof'}
            raw = (json.dumps(doc, sort_keys=True, separators=(',', ':'), allow_nan=False) + '\n').encode()
            if len(raw) > CAP:
                raise ValueError('failure carrier cap')
            frame = FAILURE_PREFIX + raw
            self.carrier['bytes'], self.carrier['sha256'] = len(frame), hashlib.sha256(frame).hexdigest()
            n = self.failure_stream.write(frame)  # one attempt, no fallback/retry on broken stderr
            self.carrier['write_return'] = n if type(n) is int and -1 <= n <= FAILURE_CAP else None
            if type(n) is not int or n != len(frame):
                raise ValueError('partial failure carrier')
            self.failure_stream.flush()
            self.carrier['flushed'] = self.carrier['complete'] = True
        except BaseException as exc:
            self.fail('failure-carrier-stream', exc)
            raise

    def finish(self, operational=None):
        if self.finished:
            self.fail('terminal-repeated')
            raise RuntimeError('terminal repeated')
        self.finished = True
        state = self.gate_failure()
        if operational is not None:
            self.fail('front-operational', operational)
        if state and (not state['ready'] or state['first_cause'] or
                      not all(w['complete'] for w in state['witnesses'])):
            self.fail('E3_refused_or_incomplete')
        doc = {'schema': 'c3.integration.e3-terminal.v1', 'token': self.binding['token'], 'T': self.T,
               'required': self.required, 'state': state, 'first_failure': self.first,
               'secondary': list(self.secondary), 'terminal_stage': 'prepared-for-persistence'}
        original = None
        try:
            raw = (json.dumps(doc, sort_keys=True, separators=(',', ':'), allow_nan=False) + '\n').encode()
            self.persist(raw)
        except BaseException as exc:
            original = exc
            if self.persistence is None:
                self.fail('terminal-serialize', exc)
        if self.first:
            try:
                self.emit_failure(state)
            except BaseException:
                if original is None:
                    raise
            if original is not None:
                raise original
            raise RuntimeError('E3 terminal refused')
        return doc
