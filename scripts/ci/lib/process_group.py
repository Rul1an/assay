#!/usr/bin/env python3
"""Signal a POSIX process group, or prove it is gone.

kill(2) on a group answers EPERM, not ESRCH, on macOS when every remaining
member is a zombie. Three states produce that EPERM:

  A. the leader exited and this process has not reaped it: only we can clear
     that zombie, so waiting alone never ends the EPERM;
  B. the leader is reaped and orphaned members await launchd: the EPERM turns
     into ESRCH on its own, within milliseconds;
  C. a live member this process may not signal: the EPERM persists.

signal_process_group reaps the leader it was given (A), retries for a bounded
drain window (B), and raises the EPERM that outlives it (C). It returns only
when the signal landed or the group is proven absent (ESRCH); EPERM is never
read as a clean stop.

Copies that cannot import this module (the digest-pinned published-release
harness, the release-shipped MCP quickstart, the self-copying Claude plugin
workflow) must match it statement for
statement; scripts/ci/test_process_group_parity.py holds them to it.
"""

from __future__ import annotations

import os
import subprocess
import time


def signal_process_group(
    pgid: int,
    signum: int,
    leader: subprocess.Popen | None = None,
    drain_seconds: float = 1.0,
) -> None:
    deadline = time.monotonic() + drain_seconds
    while True:
        try:
            os.killpg(pgid, signum)
            return
        except ProcessLookupError:
            return
        except PermissionError:
            if leader is not None:
                leader.poll()
            if time.monotonic() >= deadline:
                raise
            time.sleep(0.01)
