"""Bounded read-only directory inputs. Archives and links are not accepted."""
import json
import math
import os
import stat
from dataclasses import dataclass
from pathlib import Path


class Refused(ValueError):
    """Incomplete, malformed, or unbound evidence; no verification result."""


def need(ok, reason):
    if not ok:
        raise Refused(reason)


@dataclass(frozen=True)
class Limits:
    file_bytes: int = 4 * 1024 * 1024
    total_bytes: int = 64 * 1024 * 1024
    files: int = 10000
    path_bytes: int = 512
    depth: int = 64
    tokens: int = 250000


DEFAULT_LIMITS = Limits()


def loads(raw, limits=DEFAULT_LIMITS):
    need(len(raw) <= limits.file_bytes, 'JSON byte limit')
    depth = tokens = 0
    quoted = escaped = False
    # Structural/token ceiling before the allocating decoder. Overcounting
    # scalar punctuation is intentional; frozen evidence is far below the cap.
    for byte in raw:
        if quoted:
            if escaped:
                escaped = False
            elif byte == 92:
                escaped = True
            elif byte == 34:
                quoted = False
        elif byte == 34:
            quoted = True
            tokens += 1
        elif byte in (123, 91):
            depth += 1
            tokens += 1
            need(depth <= limits.depth, 'JSON depth limit')
        elif byte in (125, 93):
            depth -= 1
        elif byte in (44, 58):
            tokens += 1
        need(tokens <= limits.tokens, 'JSON token limit')

    def pairs(items):
        result = {}
        for key, value in items:
            need(key not in result, 'duplicate JSON key')
            result[key] = value
        return result

    def constant(_):
        raise Refused('nonfinite JSON')

    def floating(token):
        need(len(token) <= 128, 'number token limit')
        result = float(token)
        need(math.isfinite(result), 'nonfinite number')
        return result

    try:
        text = raw.decode('utf-8')
        need(not text.startswith('\ufeff'), 'JSON BOM')
        return json.loads(text, object_pairs_hook=pairs, parse_constant=constant, parse_float=floating)
    except (UnicodeError, ValueError, RecursionError) as error:
        raise Refused('invalid bounded JSON') from error


class Store:
    """No-follow openat chain plus bounded preflight inventory; never trusts labels."""
    def __init__(self, root, limits=DEFAULT_LIMITS):
        self.limits = limits
        self.fd = None
        self.names = {}
        self.total = 0
        self.entries = 0
        try:
            # Check every ancestor, not just the final directory component.
            path = Path(os.path.abspath(root))
            fd = os.open(path.anchor, os.O_RDONLY | os.O_DIRECTORY)
            try:
                for part in path.parts[1:]:
                    child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                    os.close(fd)
                    fd = child
                self.fd = fd
            except BaseException:
                os.close(fd)
                raise
            self._scan(self.fd, '', 0)
        except (OSError, Refused) as error:
            self.close()
            raise Refused('plain bounded directory required') from error

    def _path(self, name):
        need(type(name) is str and 0 < len(name.encode('utf-8')) <= self.limits.path_bytes,
             'path length')
        parts = name.split('/')
        need(all(p not in ('', '.', '..') for p in parts) and '\\' not in name and '\x00' not in name,
             'relative path required')
        return parts

    def _scan(self, fd, prefix, depth):
        need(depth <= self.limits.depth, 'directory depth limit')
        with os.scandir(fd) as entries:
            for entry in entries:
                self.entries += 1
                need(self.entries <= self.limits.files, 'entry count limit')
                name = prefix + entry.name
                self._path(name)
                st = entry.stat(follow_symlinks=False)
                if stat.S_ISDIR(st.st_mode):
                    child = os.open(entry.name, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                    try:
                        self._scan(child, name + '/', depth + 1)
                    finally:
                        os.close(child)
                else:
                    need(stat.S_ISREG(st.st_mode), 'nonregular evidence')
                    need(len(self.names) < self.limits.files, 'file count limit')
                    need(st.st_size <= self.limits.file_bytes, 'file byte limit')
                    self.total += st.st_size
                    need(self.total <= self.limits.total_bytes, 'total byte limit')
                    self.names[name] = (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)

    def read(self, name):
        parts = self._path(name)
        need(name in self.names, 'missing evidence')
        fd = os.dup(self.fd)
        try:
            for part in parts[:-1]:
                child = os.open(part, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW, dir_fd=fd)
                os.close(fd)
                fd = child
            source = os.open(parts[-1], os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=fd)
            with os.fdopen(source, 'rb') as stream:
                before = os.fstat(stream.fileno())
                need(stat.S_ISREG(before.st_mode), 'nonregular evidence')
                identity = lambda st: (st.st_dev, st.st_ino, st.st_size, st.st_mtime_ns)
                need(identity(before) == self.names[name], 'evidence changed after scan')
                raw = stream.read(self.limits.file_bytes + 1)
                need(len(raw) <= self.limits.file_bytes and len(raw) == before.st_size,
                     'bounded read mismatch')
                need(identity(os.fstat(stream.fileno())) == identity(before), 'evidence changed during read')
                return raw
        except OSError as error:
            raise Refused('evidence read refused') from error
        finally:
            os.close(fd)

    def json(self, name):
        return loads(self.read(name), self.limits)

    def close(self):
        if self.fd is not None:
            os.close(self.fd)
            self.fd = None

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.close()
