import signal
signal.alarm(4)

# replay_native.py: replays one retained native-reader stdout, byte for byte (CLAUDE-ADAPTER-DESIGN-v6 section 5).
# The two statements above must stay first (design v6 C-3). It reads at most 131073 bytes, checks the sha256, writes
# the bytes unchanged and exits 0. Any failure exits 3, never 2: the projector special-cases exit 2.
import hashlib
import os
import sys

LIMIT = 131072


def main(argv):
    if len(argv) != 3:
        return 3
    path, digest = argv[1], argv[2]
    try:
        fd = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
        with os.fdopen(fd, 'rb') as f:
            data = f.read(LIMIT + 1)
    except OSError:
        return 3
    if len(data) > LIMIT or hashlib.sha256(data).hexdigest() != digest:
        return 3
    try:
        sys.stdout.buffer.write(data)
        sys.stdout.buffer.flush()
    except OSError:
        return 3
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
