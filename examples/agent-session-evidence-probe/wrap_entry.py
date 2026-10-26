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
               '--decision-log', str(root/'decisions.ndjson'), '--decision-log-new',
               '--request-receipts-out', str(root/'requests.ndjson'),
               '--session-stop-out', str(root/'session-stop.json'),
               '--session-context-out', str(root/'session-context.json'), '--',
               '/usr/bin/python3', '-I', str(kit/'fixture_server.py'), str(root/'oracle')]
    # Preserve proxy diagnostics even when the agent host consumes stderr.
    fd = os.open(root/'proxy.stderr', os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    os.dup2(fd, 2)
    os.close(fd)
    os.execv(str(assay), command)


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, IndexError):
        print('wrapper_failed', file=sys.stderr)
        raise SystemExit(1)
