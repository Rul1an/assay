#!/usr/bin/env python3
"""Run a pre-push hook's command under a deadline that holds for the command's own process group.

Usage: run-with-timeout.py SECONDS COMMAND [ARG...]

POSIX only. The command runs in a new session, so it and the descendants that stay in its process
group share one group. At the deadline that group gets SIGTERM, and once the first process has
exited or RUN_WITH_TIMEOUT_GRACE seconds have passed (default 5), SIGKILL, so a descendant that
ignores SIGTERM is stopped too. GNU `timeout -k` does not do that: when its child exits on SIGTERM
it reports 124 and sends no SIGKILL, and the descendant keeps running. No timeout(1) or
gtimeout(1) is needed. A descendant that leaves the group (setsid, setpgid) is outside it: this is
not a guarantee about such processes.

Output is not captured: the command writes straight to this process's stdout and stderr.

Exit status: the command's own (128+N when signal N ended it); 124 at the deadline; 125 when the
group could not be signalled or the first process was not reaped within the grace, so a failed
cleanup never reads as a clean run or a plain timeout; 2 for an invalid deadline or grace; 127 or
126 when the command cannot be started. When this process gets SIGTERM, SIGINT or SIGHUP, or its
parent dies, it stops the command's group the same way and exits 128+N (129 for a lost parent).
Such a signal already recorded at the check just before the command starts means the command is
not started. That check and the start are not one atomic step: a signal recorded after it is
handled once the command runs, as before.

It leaves its parent's process group first, so a group SIGKILL aimed at the caller (a supervisor
stopping a push) cannot kill it and orphan the command. For the same reason a terminal's Ctrl+C
does not reach it: the caller must pass SIGINT, SIGTERM and SIGHUP on, as both pre-push hooks do
(run-with-timeout.sh passes SIGINT on as SIGTERM: bash starts the wrapper with SIGINT ignored).

The parent it watches is RUN_WITH_TIMEOUT_PARENT, the caller's pid, when that is set (as
run-with-timeout.sh does), and otherwise the parent it finds at startup. Only the first sees a
caller that died before this process started up: by then the parent found is a replacement. A
caller already gone means the command is never started (129). A value that is not a process id
is refused (2). The variable is not passed on to the command.

The group signal is scripts/ci/lib/process_group.py's signal_process_group, so a macOS group of
zombies (EPERM) is never mistaken for a stopped one.
"""

from __future__ import annotations

import math
import os
import re
import runpy
import signal
import subprocess
import sys
import time
from pathlib import Path

sys.dont_write_bytecode = True

LIB = Path(__file__).resolve().parents[1] / "ci" / "lib" / "process_group.py"
POLL_SECONDS = 0.05
DEFAULT_GRACE = "5"
LOST_PARENT = 128 + signal.SIGHUP
CLEANUP_FAILED = 125


def seconds(text: str) -> float | None:
    try:
        value = float(text)
    except ValueError:
        return None
    return value if math.isfinite(value) and value > 0 else None


def signal_command(process: subprocess.Popen, signum: int) -> None:
    """Signal the command's whole process group, never only its first process."""
    signal_process_group(process.pid, signum, process)


def send(process: subprocess.Popen, signum: int) -> bool:
    """Signal the group; False, said on stderr, when a member could not be signalled."""
    try:
        signal_command(process, signum)
        return True
    except PermissionError as error:
        print(f"run-with-timeout: could not send {signal.Signals(signum).name} to the command's "
              f"process group: {error}", file=sys.stderr)
        return False


def stop(process: subprocess.Popen, grace: float) -> bool:
    """SIGTERM, then SIGKILL, then reap the first process, each step bounded; True when all held."""
    clean = send(process, signal.SIGTERM)
    end = time.monotonic() + grace
    while process.poll() is None and time.monotonic() < end:
        time.sleep(POLL_SECONDS)
    # Whether or not the first process is gone: a descendant may have ignored SIGTERM.
    clean = send(process, signal.SIGKILL) and clean
    try:
        process.wait(timeout=grace)
    except subprocess.TimeoutExpired:
        print(f"run-with-timeout: the command's first process (pid {process.pid}) did not exit; "
              f"it and its group may still be running", file=sys.stderr)
        clean = False
    return clean


def main(argv: list[str]) -> int:
    if len(argv) < 2:
        print("usage: run-with-timeout.py SECONDS COMMAND [ARG...]", file=sys.stderr)
        return 2
    deadline_seconds = seconds(argv[0])
    if deadline_seconds is None:
        print(f"run-with-timeout: the deadline must be a positive number of seconds, not {argv[0]!r}",
              file=sys.stderr)
        return 2
    grace = seconds(os.environ.get("RUN_WITH_TIMEOUT_GRACE", DEFAULT_GRACE))
    if grace is None:
        print("run-with-timeout: RUN_WITH_TIMEOUT_GRACE must be a positive number of seconds",
              file=sys.stderr)
        return 2
    given = os.environ.pop("RUN_WITH_TIMEOUT_PARENT", None)
    # A process id has at most 10 digits here, which keeps int() far from its digit limit.
    if given is not None and re.fullmatch(r"[1-9][0-9]{0,9}", given) is None:
        print(f"run-with-timeout: RUN_WITH_TIMEOUT_PARENT must be the caller's process id, not "
              f"{given[:20]!r}", file=sys.stderr)
        return 2

    # The caller's pid, when given, also covers a caller lost before this process started up.
    # Otherwise the parent found now, before leaving the caller's group: a caller lost during that
    # step must not become the parent the loop compares against.
    parent = int(given) if given is not None else os.getppid()
    try:
        os.setpgid(0, 0)
    except OSError:
        pass  # already a session or group leader
    # In a background group a write to the terminal could stop this process (TOSTOP).
    signal.signal(signal.SIGTTOU, signal.SIG_IGN)
    received: list[int] = []
    for signum in (signal.SIGTERM, signal.SIGINT, signal.SIGHUP):
        signal.signal(signum, lambda number, _frame: received.append(number))

    if os.getppid() != parent:
        print("run-with-timeout: the caller is gone; the command was not started", file=sys.stderr)
        return LOST_PARENT
    if received:
        # Already recorded here, during startup: there is nothing to stop yet, so do not start it.
        # A signal recorded after this check and before Popen is handled by the loop below.
        return 128 + received[0]
    try:
        process = subprocess.Popen(argv[1:], start_new_session=True)
    except FileNotFoundError:
        print(f"run-with-timeout: command not found: {argv[1]}", file=sys.stderr)
        return 127
    except OSError as error:
        print(f"run-with-timeout: cannot run {argv[1]}: {error}", file=sys.stderr)
        return 126

    end = time.monotonic() + deadline_seconds
    while True:
        status = process.poll()
        if status is not None:
            # Anything still in the command's group is the command's, and must not outlive it.
            if not send(process, signal.SIGKILL):
                print("run-with-timeout: processes the command left in its group may still be running",
                      file=sys.stderr)
                return CLEANUP_FAILED
            return 128 - status if status < 0 else status
        if received:
            return 128 + received[0] if stop(process, grace) else CLEANUP_FAILED
        if os.getppid() != parent:
            return LOST_PARENT if stop(process, grace) else CLEANUP_FAILED
        if time.monotonic() >= end:
            print(f"run-with-timeout: deadline of {deadline_seconds:g}s reached; stopping the "
                  f"command's process group (SIGTERM, then SIGKILL)", file=sys.stderr, flush=True)
            return 124 if stop(process, grace) else CLEANUP_FAILED
        time.sleep(POLL_SECONDS)


signal_process_group = runpy.run_path(str(LIB))["signal_process_group"]

if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
