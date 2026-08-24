#!/usr/bin/env python3
"""One bounded child-process runner, shared by the conformance tools.

Standard library only. Extracted so the output ceiling has a single
implementation: two copies of a resource ceiling drift, and the copy that
drifts is the one that stops enforcing it.
"""

from __future__ import annotations

import os
import select
import signal
import subprocess
import threading
import time
from pathlib import Path


# Output ceiling per child process. Both children can emit arbitrary output and
# a timeout does not bound memory, so the cap is applied while the process runs
# rather than after it exits.
OUTPUT_CAP_BYTES = 4 * 1024 * 1024

# One fixed read size. Charge-before-retain keeps combined stdout+stderr at
# most OUTPUT_CAP_BYTES. The two reader threads may each already hold a
# chunk before that lock runs, so in-flight (not yet retained) bytes stay at
# most 2 * READ_CHUNK_BYTES. Reads themselves are not locked: serializing a
# blocking read can stall the other pipe.
READ_CHUNK_BYTES = 64 * 1024
READ_WAIT_SECONDS = 0.05
READER_JOIN_TIMEOUT_SECONDS = 1.0


class _OutputTooLarge(Exception):
    """A child exceeded OUTPUT_CAP_BYTES; its output is not materialized."""


class _OutputDrainIncomplete(OSError):
    """The child exited, but its captured output streams did not reach EOF."""


def _charge_before_retain(total: int, cap: int, data: bytes) -> tuple[bytes, bool]:
    """Accept at most the remaining allowance. Overflowing bytes are dropped.

    Exact cap is not overflow. The first byte past cap is not retained.
    """
    room = cap - total
    if len(data) > room:
        return data[:room], True
    return data, False


def _posix_process_group() -> bool:
    return hasattr(os, "killpg") and hasattr(os, "setsid")


def _run_capped(cmd: list[str], cwd: Path, timeout: int) -> subprocess.CompletedProcess:
    """subprocess.run with a ceiling on how much output is ever held.

    Drains stdout and stderr concurrently through pipes. One locked
    charge-before-retain keeps combined retained bytes at most
    OUTPUT_CAP_BYTES. Crossing the cap kills the POSIX process group and
    raises _OutputTooLarge. In-flight read buffers may briefly hold
    2 * READ_CHUNK_BYTES more. Timeout remains TimeoutExpired. A child that
    exits while an escaped descendant retains the streams raises
    _OutputDrainIncomplete. No temporary output files are used.

    Windows: process/batch already refuse where fcntl is missing, and this
    helper kills only the direct child. That is not a process-tree claim.
    """
    cap = OUTPUT_CAP_BYTES
    posix = _posix_process_group()
    kwargs = {
        "cwd": str(cwd),
        "stdout": subprocess.PIPE,
        "stderr": subprocess.PIPE,
    }
    if posix:
        kwargs["start_new_session"] = True
    proc = subprocess.Popen(cmd, **kwargs)
    pgid = proc.pid

    lock = threading.Lock()
    stop = threading.Event()
    total = 0
    overflow = False
    timed_out = False
    killed = False
    chunks = {1: [], 2: []}
    reader_error: list[BaseException] = []

    def kill_boundary() -> None:
        nonlocal killed
        with lock:
            if killed:
                return
            killed = True
        if posix:
            try:
                os.killpg(pgid, signal.SIGKILL)
            except (ProcessLookupError, PermissionError, OSError):
                try:
                    proc.kill()
                except OSError:
                    pass
        else:
            try:
                proc.kill()
            except OSError:
                pass
        try:
            proc.wait(timeout=10)
        except subprocess.TimeoutExpired:
            pass

    def charge(which: int, data: bytes) -> None:
        nonlocal total, overflow
        with lock:
            if overflow:
                return
            data, overflowed = _charge_before_retain(total, cap, data)
            if data:
                chunks[which].append(data)
                total += len(data)
            if overflowed:
                overflow = True
                stop.set()

    def reader(stream, which: int) -> None:
        try:
            while not stop.is_set():
                if posix and hasattr(stream, "fileno") and hasattr(stream, "read1"):
                    readable, _, _ = select.select(
                        [stream.fileno()], [], [], READ_WAIT_SECONDS
                    )
                    if not readable:
                        continue
                    data = stream.read1(READ_CHUNK_BYTES)
                else:
                    data = stream.read(READ_CHUNK_BYTES)
                if not data:
                    break
                charge(which, data)
                if overflow:
                    kill_boundary()
                    break
        except Exception as exc:  # noqa: BLE001 - surface after join
            reader_error.append(exc)
            stop.set()
            kill_boundary()
        finally:
            try:
                stream.close()
            except OSError:
                pass

    threads = (
        threading.Thread(
            target=reader, args=(proc.stdout, 1), name="bounded-run-stdout", daemon=True
        ),
        threading.Thread(
            target=reader, args=(proc.stderr, 2), name="bounded-run-stderr", daemon=True
        ),
    )
    for thread in threads:
        thread.start()

    deadline = time.monotonic() + timeout
    incomplete_drain = False
    try:
        while True:
            returncode = proc.poll()
            if overflow:
                break
            if time.monotonic() > deadline:
                timed_out = True
                break
            if reader_error:
                break
            if returncode is not None:
                break
            try:
                proc.wait(timeout=0.05)
                break
            except subprocess.TimeoutExpired:
                pass
    finally:
        if overflow or timed_out or reader_error:
            stop.set()
            kill_boundary()
        else:
            kill_boundary()
        for thread in threads:
            thread.join(timeout=READER_JOIN_TIMEOUT_SECONDS)
            if thread.is_alive():
                incomplete_drain = True
                stop.set()
                kill_boundary()
        for thread in threads:
            if thread.is_alive():
                thread.join(timeout=4 * READ_WAIT_SECONDS)

    if overflow:
        raise _OutputTooLarge()
    if timed_out:
        raise subprocess.TimeoutExpired(cmd, timeout)
    if reader_error:
        raise reader_error[0]
    if incomplete_drain:
        raise _OutputDrainIncomplete("capped child output did not reach EOF")
    stdout_text = b"".join(chunks[1]).decode("utf-8", "replace")
    stderr_text = b"".join(chunks[2]).decode("utf-8", "replace")
    return subprocess.CompletedProcess(
        cmd, proc.returncode, stdout_text, stderr_text
    )
