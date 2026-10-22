"""One proxy start per attempt; a reconnect is a retained incomplete attempt."""
import os
from pathlib import Path
import sys


def claim_start(root):
    try:
        with (Path(root) / 'wrap.started').open('xb') as out:
            out.write(b'first_start\n')
    except FileExistsError:
        try:
            with (Path(root) / 'wrap.restart').open('xb') as out:
                out.write(b'incomplete_restart\n')
        except FileExistsError:
            pass
        raise


def main():
    root = Path(sys.argv[1]).resolve()
    assay = Path(sys.argv[2]).resolve(strict=True)
    kit = Path(__file__).resolve().parent
    claim_start(root)
    command = [str(assay), 'mcp', 'wrap', '--policy', str(kit/'policy.yaml'),
               '--event-source', 'assay://agent-session-probe',
               '--decision-log', str(root/'decisions.ndjson'),
               '--session-context-out', str(root/'session-context.json'), '--',
               '/usr/bin/python3', '-I', str(kit/'fixture_server.py'), str(root/'oracle')]
    os.execv(str(assay), command)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, IndexError):
        print('wrapper_failed', file=sys.stderr)
        raise SystemExit(1)
