"""Bounded read-only evidence inputs.

Store reads a plain directory tree and refuses links. ArchiveStore reads one
committed ustar+gzip archive by unpacking it, under the same limits and path
rules, into a private directory that Store then reads; the archive's members
are never trusted, and the unpacked tree is removed afterwards.
"""
import gzip
import json
import math
import os
import re
import shutil
import stat
import tempfile
import zlib
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


def relative_parts(name, limits=DEFAULT_LIMITS):
    """The one path rule for directory entries and archive members alike."""
    need(type(name) is str and 0 < len(name.encode('utf-8')) <= limits.path_bytes,
         'path length')
    parts = name.split('/')
    need(all(p not in ('', '.', '..') for p in parts) and '\\' not in name and '\x00' not in name,
         'relative path required')
    need(len(parts) <= limits.depth, 'directory depth limit')
    return parts


class Store:
    """No-follow openat chain plus bounded preflight inventory; never trusts labels."""
    def __init__(self, root, limits=DEFAULT_LIMITS):
        self.limits = limits
        self.root = Path(os.path.abspath(root))
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
        return relative_parts(name, self.limits)

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


# Archive format: POSIX ustar, regular-file members only, inside one gzip member.
# Directories are implied by member paths; links, pax/GNU extensions, device
# entries and directory members are refused rather than interpreted.
BLOCK = 512
USTAR_MAGIC = b'ustar\x0000'
_OCTAL = re.compile(rb'[0-7]{1,11}[\x00 ]*')


def _expansion_cap(limits):
    # Member bytes, one header and at most one padding block per file, the
    # two end blocks, and one 10240-byte record of trailing zero padding.
    return limits.total_bytes + 2 * BLOCK * (limits.files + 2) + 20 * BLOCK


def _gzip_bound(n):
    """Largest gzip member deflate can produce from n input bytes.

    zlib's deflateBound() for default windowBits and memLevel (which Python's
    gzip uses at every level) with the 18-byte gzip wrapper (a 10-byte header
    without optional fields, an 8-byte trailer): stored blocks add 5 bytes per
    16 KB (zlib technical notes). Identical in zlib 1.2.12 and 1.3.1.
    """
    return n + (n >> 12) + (n >> 14) + (n >> 25) + 13 - 6 + 18


class _Inflate:
    """gzip stream decompressed in bounded steps; refuses past the expansion cap."""
    def __init__(self, fd, cap):
        self.fd = fd
        self.cap = cap
        self.out = 0
        self.buf = bytearray()
        self.d = zlib.decompressobj(16 + zlib.MAX_WBITS)
        self.done = False

    def _step(self):
        data = self.d.unconsumed_tail
        if not data:
            data = os.read(self.fd, 64 * 1024)
            if not data:
                need(self.d.eof, 'truncated compressed stream')
                self.done = True
                return
        chunk = self.d.decompress(data, 64 * 1024)
        self.out += len(chunk)
        need(self.out <= self.cap, 'decompressed size limit')
        self.buf += chunk
        if self.d.eof:
            need(not self.d.unused_data and not self.d.unconsumed_tail and not os.read(self.fd, 1),
                 'data after the compressed stream')
            self.done = True

    def read(self, n):
        while len(self.buf) < n and not self.done:
            self._step()
        out = bytes(self.buf[:n])
        del self.buf[:n]
        return out

    def exact(self, n):
        out = self.read(n)
        need(len(out) == n, 'truncated archive')
        return out


def _field(raw, what):
    end = raw.find(b'\x00')
    if end >= 0:
        need(not raw[end:].strip(b'\x00'), what + ' field padding')
        raw = raw[:end]
    return raw


def _octal(raw, what):
    need(_OCTAL.fullmatch(raw) is not None, what + ' field')
    return int(raw.rstrip(b'\x00 '), 8)


def _member(block):
    stored = block[148:156].strip(b' \x00')
    need(re.fullmatch(rb'[0-7]{1,7}', stored) is not None, 'checksum field')
    # The checksum is computed with its own field read as eight spaces.
    need(int(stored, 8) == sum(block[:148]) + 8 * 32 + sum(block[156:]), 'header checksum')
    need(block[257:265] == USTAR_MAGIC, 'ustar header required')
    need(block[156:157] in (b'0', b'\x00'), 'regular file member required')
    try:
        name = _field(block[0:100], 'name').decode('utf-8')
        prefix = _field(block[345:500], 'prefix').decode('utf-8')
    except UnicodeError as error:
        raise Refused('member name encoding') from error
    if prefix:
        name = prefix + '/' + name
    return name, _octal(block[124:136], 'size')


def unpack(archive, dest, limits=DEFAULT_LIMITS):
    """Unpack one archive into a new directory, or leave nothing behind.

    The destination must not exist. Every member is checked before its bytes
    are written, and decompression is bounded on the stream, so an expansion
    past the limits refuses before it is materialized.
    """
    dest = Path(os.path.abspath(dest))
    try:
        os.mkdir(dest, 0o700)
    except OSError as error:
        raise Refused('archive destination must be new') from error
    try:
        _unpack_into(Path(archive), dest, limits)
    except BaseException as error:
        shutil.rmtree(dest, ignore_errors=True)
        if isinstance(error, (OSError, zlib.error)):
            raise Refused('archive refused') from error
        raise


def _unpack_into(archive, dest, limits):
    nofollow = getattr(os, 'O_NOFOLLOW', None)
    need(nofollow is not None, 'no O_NOFOLLOW')
    fd = os.open(archive, os.O_RDONLY | nofollow | os.O_NONBLOCK)
    try:
        st = os.fstat(fd)
        need(stat.S_ISREG(st.st_mode), 'archive must be a regular file')
        cap = _expansion_cap(limits)
        # No gzip member of at most `cap` decompressed bytes is larger than this,
        # so a larger file is refused before anything is decompressed.
        need(st.st_size <= _gzip_bound(cap), 'archive byte limit')
        stream = _Inflate(fd, cap)
        files, dirs, total = set(), set(), 0
        while True:
            block = stream.exact(BLOCK)
            if not block.strip(b'\x00'):
                need(not stream.exact(BLOCK).strip(b'\x00'), 'single end-of-archive block')
                while True:
                    rest = stream.read(64 * 1024)
                    if not rest:
                        break
                    need(not rest.strip(b'\x00'), 'data after end of archive')
                break
            name, size = _member(block)
            parts = relative_parts(name, limits)
            need(name not in files and name not in dirs, 'duplicate or overlapping member')
            ancestors = ['/'.join(parts[:i]) for i in range(1, len(parts))]
            need(not any(a in files for a in ancestors), 'member below a file member')
            need(size <= limits.file_bytes, 'file byte limit')
            total += size
            need(total <= limits.total_bytes, 'total byte limit')
            new_dirs = [a for a in ancestors if a not in dirs]
            need(len(files) + len(dirs) + len(new_dirs) + 1 <= limits.files, 'entry count limit')
            for a in new_dirs:
                os.mkdir(dest / a, 0o755)
                dirs.add(a)
            out = os.open(dest / name, os.O_WRONLY | os.O_CREAT | os.O_EXCL | nofollow, 0o644)
            try:
                left = size
                while left:
                    chunk = memoryview(stream.exact(min(left, 64 * 1024)))
                    left -= len(chunk)
                    while chunk:
                        written = os.write(out, chunk)
                        need(written > 0, 'short write')
                        chunk = chunk[written:]
            finally:
                os.close(out)
            files.add(name)
            stream.exact(-size % BLOCK)
        need(files, 'empty archive')
    finally:
        os.close(fd)


class ArchiveStore:
    """Store over a private, bounded unpack of one archive; removed on exit."""
    def __init__(self, archive, limits=DEFAULT_LIMITS):
        self.archive = archive
        self.limits = limits
        self.scratch = None
        self.store = None

    def __enter__(self):
        # Resolved once: Store refuses a linked ancestor, and the system temp dir
        # can sit behind one (macOS /var). This directory is ours, just created.
        self.scratch = os.path.realpath(tempfile.mkdtemp(prefix='refund-retained-'))
        try:
            tree = Path(self.scratch) / 'tree'
            unpack(self.archive, tree, self.limits)
            self.store = Store(tree, self.limits)
            return self.store
        except BaseException:
            self.__exit__()
            raise

    def __exit__(self, *_):
        if self.store is not None:
            self.store.close()
            self.store = None
        if self.scratch is not None:
            shutil.rmtree(self.scratch, ignore_errors=True)
            self.scratch = None


def open_retained(path, limits=DEFAULT_LIMITS):
    """A plain directory reads as before; anything else must be the archive."""
    try:
        is_dir = stat.S_ISDIR(os.lstat(path).st_mode)
    except OSError as error:
        raise Refused('plain bounded directory or archive required') from error
    return Store(path, limits) if is_dir else ArchiveStore(path, limits)


def _ustar_header(name, size):
    raw = name.encode('utf-8')
    prefix = b''
    if len(raw) > 100:
        # Split at a '/' so the prefix fits 155 bytes and the rest fits 100.
        cuts = [i for i, b in enumerate(raw) if b == 0x2F and i <= 155 and len(raw) - i - 1 <= 100]
        need(bool(cuts), 'member name does not fit ustar')
        prefix, raw = raw[:cuts[0]], raw[cuts[0] + 1:]
    block = bytearray(BLOCK)
    block[0:len(raw)] = raw
    block[100:108] = b'0000644\x00'
    block[108:116] = block[116:124] = b'0000000\x00'
    block[124:136] = b'%011o\x00' % size
    block[136:148] = b'00000000000\x00'
    block[156:157] = b'0'
    block[257:265] = USTAR_MAGIC
    block[345:345 + len(prefix)] = prefix
    block[148:156] = b' ' * 8
    block[148:156] = b'%06o\x00 ' % sum(block)
    return bytes(block)


def pack(root, out, limits=DEFAULT_LIMITS):
    """Write root as a deterministic archive: sorted names, fixed metadata, no timestamps.

    The tree is read through Store, so links and oversized inputs are refused
    exactly as on the read side.
    """
    with Store(root, limits) as store, open(out, 'xb') as raw:
        with gzip.GzipFile(filename='', mode='wb', fileobj=raw, compresslevel=9, mtime=0) as stream:
            for name in sorted(store.names, key=lambda n: n.encode('utf-8')):
                data = store.read(name)
                stream.write(_ustar_header(name, len(data)))
                stream.write(data)
                stream.write(b'\x00' * (-len(data) % BLOCK))
            stream.write(b'\x00' * (2 * BLOCK))
