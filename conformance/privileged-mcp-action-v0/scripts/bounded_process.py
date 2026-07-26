#!/usr/bin/env python3
"""Run a child process with hard timeout and output ceilings."""

from __future__ import annotations

import os
import signal
import subprocess
import sys
import threading
import time
from collections.abc import Sequence
from dataclasses import dataclass
from pathlib import Path
from typing import BinaryIO


@dataclass(frozen=True)
class BoundedResult:
    returncode: int
    stdout: bytes
    stderr: bytes


class ProcessLimitError(RuntimeError):
    pass


class ProcessCaptureError(ProcessLimitError):
    pass


def _terminate_and_reap(process: subprocess.Popen[bytes]) -> None:
    if os.name == "posix":
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except PermissionError:
            # macOS reports EPERM when only the unsignalable zombie leader
            # remains in the group. WNOWAIT still keeps its PID unrecycled.
            if sys.platform != "darwin" or not _leader_exited_without_reaping(process):
                raise
        except ProcessLookupError:
            pass
    elif process.poll() is None:
        process.kill()
    process.wait()


def _leader_exited_without_reaping(process: subprocess.Popen[bytes]) -> bool:
    if os.name != "posix":
        return process.poll() is not None
    # Keep the leader as a zombie until its process group is swept. That keeps
    # its PID from being recycled before killpg targets the original group.
    return (
        os.waitid(
            os.P_PID,
            process.pid,
            os.WEXITED | os.WNOHANG | os.WNOWAIT,
        )
        is not None
    )


def _capture_stream(stream: BinaryIO, limit: int, exceeded: threading.Event) -> bytes:
    collected = bytearray()
    while True:
        chunk = stream.read(min(64 * 1024, limit + 1 - len(collected)))
        if not chunk:
            break
        collected.extend(chunk)
        if len(collected) > limit:
            exceeded.set()
            break
    return bytes(collected)


def _close_pipes(process: subprocess.Popen[bytes]) -> None:
    if process.stdout is not None:
        process.stdout.close()
    if process.stderr is not None:
        process.stderr.close()


def run_bounded(
    command: Sequence[str],
    *,
    timeout_seconds: int,
    stdout_limit: int,
    stderr_limit: int,
    cwd: Path | None = None,
) -> BoundedResult:
    if timeout_seconds <= 0:
        raise ValueError("timeout_seconds must be positive")
    if stdout_limit < 0 or stderr_limit < 0:
        raise ValueError("output limits must be non-negative")

    # This helper intentionally runs the caller-supplied argv without a shell. It
    # bounds I/O and the initial POSIX process group, but it is not a sandbox:
    # a child that creates another group leaves that operational boundary.
    process = subprocess.Popen(
        command,
        cwd=cwd,
        stdin=subprocess.DEVNULL,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=os.name == "posix",
    )
    assert process.stdout is not None
    assert process.stderr is not None

    exceeded = threading.Event()
    capture_failed = threading.Event()
    outputs: dict[str, bytes] = {}
    capture_failures: dict[str, Exception] = {}

    def capture(name: str, stream: BinaryIO, limit: int) -> None:
        try:
            outputs[name] = _capture_stream(stream, limit, exceeded)
        except OSError as error:
            capture_failures[name] = error
            capture_failed.set()

    threads = [
        threading.Thread(
            target=capture,
            args=("stdout", process.stdout, stdout_limit),
            daemon=True,
        ),
        threading.Thread(
            target=capture,
            args=("stderr", process.stderr, stderr_limit),
            daemon=True,
        ),
    ]
    for thread in threads:
        thread.start()

    deadline = time.monotonic() + timeout_seconds
    while True:
        if exceeded.is_set() or capture_failed.is_set():
            _terminate_and_reap(process)
            break
        if time.monotonic() >= deadline:
            _terminate_and_reap(process)
            for thread in threads:
                thread.join(timeout=1)
            _close_pipes(process)
            raise ProcessLimitError(f"process timed out after {timeout_seconds}s")
        if _leader_exited_without_reaping(process):
            _terminate_and_reap(process)
            break
        time.sleep(0.01)

    # A candidate may let its leader exit while descendants retain the capture
    # pipes or continue in the background. The process group was swept before
    # the leader was reaped, so process.pid cannot have been recycled.
    for thread in threads:
        thread.join(timeout=1)
    if capture_failures:
        _close_pipes(process)
        failed_streams = ", ".join(sorted(capture_failures))
        raise ProcessCaptureError(f"process output capture failed: {failed_streams}")
    if exceeded.is_set():
        _close_pipes(process)
        raise ProcessLimitError("process output exceeded its byte ceiling")
    if any(thread.is_alive() for thread in threads):
        _close_pipes(process)
        raise ProcessLimitError("process output capture did not terminate")
    result = BoundedResult(
        returncode=process.returncode,
        stdout=outputs["stdout"],
        stderr=outputs["stderr"],
    )
    _close_pipes(process)
    return result
