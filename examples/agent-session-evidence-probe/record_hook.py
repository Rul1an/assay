"""Passive byte recorder for the bounded experiment, not an evidence importer."""
import collections
import json
import hashlib
import stat
import os
from pathlib import Path
import sys

PROTOCOL = json.loads(Path(__file__).with_name('protocol.json').read_text())


def capture(stream, output_dir, max_bytes, *, max_occurrences=None):
    """Reserve before reading; incomplete original bytes survive without .ok."""
    if max_occurrences is None:
        max_occurrences = PROTOCOL['max_hook_occurrences']
    root = Path(output_dir)
    for slot in range(max_occurrences):
        path = root / f'{slot:03d}.raw'
        try:
            fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
            break
        except FileExistsError:
            continue
    else:
        raise ValueError('occurrence_limit')
    count = 0
    digest = hashlib.sha256()
    with os.fdopen(fd, 'wb') as out:
        while True:
            part = stream.read(min(65536, max_bytes + 1 - count))
            if not part:
                break
            out.write(part)
            digest.update(part)
            count += len(part)
            if count > max_bytes:
                raise ValueError('input_limit')
        out.flush()
        os.fsync(out.fileno())
    # Marker is separate; it never changes the original payload.
    with path.with_suffix('.ok').open('xb') as marker:
        marker.write(json.dumps({'bytes': count, 'sha256': digest.hexdigest()}).encode() + b'\n')
    return path


def _pairs(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ValueError('duplicate_key')
        result[key] = value
    return result


def _nonfinite(_):
    raise ValueError('non_finite')


def _read(path, limit):
    fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK)
    with os.fdopen(fd, 'rb') as stream:
        info = os.fstat(stream.fileno())
        if not stat.S_ISREG(info.st_mode) or info.st_size > limit:
            raise ValueError('file_shape')
        raw = stream.read(limit + 1)
        if len(raw) > limit:
            raise ValueError('input_limit')
        return raw


def reconcile(starts, responses, output_dir, *, expected, final_seen, wrapper_starts):
    """Producer-reported consistency only; measured event-key extraction is external.

    Keys are (hook_event_name, tool_use_id). Never pass counts/tool IDs alone.
    Missing native lifecycle keys preclude this check, not justify invented keys.
    """
    try:
        captured = []
        root = Path(output_dir)
        members = set()
        with os.scandir(root) as entries:
            for entry in entries:
                if len(members) >= 2 * PROTOCOL['max_hook_occurrences']:
                    return False
                members.add(entry.name)
        expected_members = set()
        for slot in range(PROTOCOL['max_hook_occurrences']):
            path = root / f'{slot:03d}.raw'
            marker = path.with_suffix('.ok')
            if path.name not in members:
                continue
            if slot != len(captured):
                return False
            raw = _read(path, PROTOCOL['max_hook_bytes'])
            proof = json.loads(_read(marker, 256), object_pairs_hook=_pairs, parse_constant=_nonfinite)
            if proof != {'bytes': len(raw), 'sha256': hashlib.sha256(raw).hexdigest()}:
                return False
            obj = json.loads(raw, object_pairs_hook=_pairs, parse_constant=_nonfinite)
            key = (obj.get('hook_event_name'), obj.get('tool_use_id'))
            if key[0] not in PROTOCOL['hook_events'] or not isinstance(key[1], str) or not key[1]:
                return False
            captured.append(key)
            expected_members.update((path.name, marker.name))
        if members != expected_members:
            return False
        for group in (starts, responses, expected):
            if not group or any(not isinstance(key, tuple) or len(key) != 2
                                or key[0] not in PROTOCOL['hook_events']
                                or not isinstance(key[1], str) or not key[1] for key in group):
                return False
        return (final_seen is True and wrapper_starts == 1
                and collections.Counter(starts) == collections.Counter(responses)
                == collections.Counter(captured) == collections.Counter(expected))
    except (OSError, ValueError, TypeError, AttributeError, RecursionError):
        return False


def main():
    # A recorder failure must not become a policy deny. The outer experiment must
    # also reconcile vendor lifecycle events: this diagnostic can itself be lost.
    try:
        capture(sys.stdin.buffer, Path(sys.argv[1]), PROTOCOL['max_hook_bytes'])
    except (OSError, ValueError, IndexError):
        print('capture_failed', file=sys.stderr)
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
