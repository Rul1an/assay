#!/usr/bin/env python3
"""One bounded child-process runner, shared by the conformance tools.

Standard library only. Extracted so the output ceiling has a single
implementation: two copies of a resource ceiling drift, and the copy that
drifts is the one that stops enforcing it.
"""

from __future__ import annotations

import os
import signal
import subprocess
import tempfile
import time
from pathlib import Path


# Output ceiling per child process. Both children can emit arbitrary output and
# a timeout does not bound memory, so the cap is applied while the process runs
# rather than after it exits.
OUTPUT_CAP_BYTES = 4 * 1024 * 1024


class _OutputTooLarge(Exception):
    """A child exceeded OUTPUT_CAP_BYTES; its output is not materialized."""


def _run_capped(cmd: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess:
    """subprocess.run with a ceiling on how much output is ever held.

    Streams both pipes to temporary files, polls their combined size while the
    child runs, and kills it the moment the cap is crossed. Only the first
    OUTPUT_CAP_BYTES are ever read into memory.
    """
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        # A descendant that inherits stdout/stderr keeps writing after proc.kill()
        # and walks straight through the ceiling, so the child leads its own session
        # and the whole group is stopped and reaped.
        proc = subprocess.Popen(cmd, cwd=str(cwd), stdout=out, stderr=err,
                                start_new_session=True)

        def _kill_tree() -> None:
            try:
                os.killpg(os.getpgid(proc.pid), signal.SIGKILL)
            except (ProcessLookupError, PermissionError, OSError):
                proc.kill()
            try:
                proc.wait(timeout=10)
            except subprocess.TimeoutExpired:
                pass
        deadline = time.monotonic() + timeout
        try:
            while True:
                try:
                    proc.wait(timeout=0.25)
                    break
                except subprocess.TimeoutExpired:
                    pass
                if out.tell() + err.tell() > OUTPUT_CAP_BYTES:
                    _kill_tree()
                    raise _OutputTooLarge()
                if time.monotonic() > deadline:
                    _kill_tree()
                    raise subprocess.TimeoutExpired(cmd, timeout)
        finally:
            # Reap the group even on the clean path: the leader can exit while a
            # descendant it spawned is still holding the inherited handles.
            _kill_tree()
        if out.tell() + err.tell() > OUTPUT_CAP_BYTES:
            raise _OutputTooLarge()
        out.seek(0)
        err.seek(0)
        return subprocess.CompletedProcess(
            cmd, proc.returncode,
            out.read(OUTPUT_CAP_BYTES).decode("utf-8", "replace"),
            err.read(OUTPUT_CAP_BYTES).decode("utf-8", "replace"),
        )
