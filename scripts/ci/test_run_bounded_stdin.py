#!/usr/bin/env python3
"""Deadline must supervise stdin delivery for run_bounded."""

from __future__ import annotations

import importlib.util
import os
import selectors
import signal
import subprocess
import sys
import tempfile
import threading
import time
import unittest
from collections.abc import Callable
from pathlib import Path
from types import ModuleType
from typing import Any
from unittest.mock import patch

ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / "scripts/ci/claude_plugin_install_workflow.py"
STDIN_256K = b"x" * 262_144
DEADLINE_S = "0.05"
SLEEP_S = 0.35
WALL_S = 1.0


def load_workflow() -> ModuleType:
    spec = importlib.util.spec_from_file_location(
        "claude_plugin_install_workflow_2193",
        WORKFLOW,
    )
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {WORKFLOW}")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


WORKFLOW_MOD = load_workflow()


def _python(body: str) -> list[str]:
    return [sys.executable, "-c", body]


def _probe_body(action: str) -> str:
    preamble = (
        "import os, sys, time\n"
        "from pathlib import Path\n"
        "Path(os.environ['BOUNDED_STDIN_PIDFILE']).write_text(str(os.getpid()))\n"
    )
    actions = {
        "non_reader": "time.sleep(0.35)\n",
        "write_first": (
            "sys.stdout.buffer.write(b'z' * 262144)\n"
            "sys.stdout.buffer.flush()\n"
            "time.sleep(0.35)\n"
        ),
        "reader": (
            "data = sys.stdin.buffer.read()\n"
            "sys.stdout.buffer.write(b'got:%d\\n' % len(data))\n"
        ),
        "early_close": (
            "os.close(0)\n"
            "raise SystemExit(0)\n"
        ),
        "flood": (
            "sys.stdout.buffer.write(b'w' * (1048576 + 1))\n"
            "sys.stdout.buffer.flush()\n"
        ),
        "true": "raise SystemExit(0)\n",
    }
    return preamble + actions[action]


class Outcome:
    def __init__(
        self,
        kind: str,
        elapsed: float,
        result: Any = None,
        reason: str = "",
    ) -> None:
        self.kind = kind
        self.elapsed = elapsed
        self.result = result
        self.reason = reason


def _reap_probe(pidfile: Path) -> None:
    try:
        raw = pidfile.read_text().strip()
        pid = int(raw)
    except (FileNotFoundError, ValueError):
        return
    for kill in (
        lambda: os.killpg(pid, signal.SIGKILL),
        lambda: os.kill(pid, signal.SIGKILL),
    ):
        try:
            kill()
        except ProcessLookupError:
            pass


def _invoke(
    runner: Callable[..., Any],
    argv: list[str],
    stdin: bytes,
    cwd: Path,
    pidfile: Path,
    wall: float = WALL_S,
) -> Outcome:
    env = WORKFLOW_MOD.clean_env(
        {"BOUNDED_STDIN_PIDFILE": str(pidfile), "PYTHONUNBUFFERED": "1"}
    )
    box: dict[str, Any] = {}

    def target() -> None:
        with patch.dict(
            os.environ,
            {"ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS": DEADLINE_S},
            clear=False,
        ):
            try:
                box["result"] = runner(
                    "bounded_stdin",
                    argv,
                    cwd=cwd,
                    env=env,
                    stdin=stdin,
                )
                box["kind"] = "ok"
            except WORKFLOW_MOD.WorkflowError as error:
                box["kind"] = "err"
                box["reason"] = error.reason
                box["result"] = error

    started = time.monotonic()
    thread = threading.Thread(target=target, daemon=True)
    thread.start()
    thread.join(wall)
    elapsed = time.monotonic() - started
    if thread.is_alive():
        _reap_probe(pidfile)
        return Outcome("hang", elapsed, reason="blocked beyond wall budget")
    if box.get("kind") == "err":
        return Outcome("err", elapsed, result=box["result"], reason=box.get("reason", ""))
    if box.get("kind") == "ok":
        return Outcome("ok", elapsed, result=box["result"])
    return Outcome("exc", elapsed, reason="runner returned no outcome")


def run_bounded_blocking_stdin_before_deadline(
    phase: str,
    argv: list[str],
    *,
    cwd: Path,
    env: dict[str, str],
    stdin: bytes = b"",
    allowed_codes: Any = (0,),
) -> Any:
    """Mutation: blocking write/flush/close before the absolute deadline."""
    timeout = WORKFLOW_MOD.workflow_timeout_seconds()
    process = subprocess.Popen(
        argv,
        cwd=cwd,
        env=env,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
    assert process.stdin is not None
    assert process.stdout is not None
    assert process.stderr is not None
    if stdin:
        try:
            process.stdin.write(stdin)
            process.stdin.flush()
        except BrokenPipeError:
            pass
    process.stdin.close()
    selector = selectors.DefaultSelector()
    selector.register(process.stdout, selectors.EVENT_READ, "stdout")
    selector.register(process.stderr, selectors.EVENT_READ, "stderr")
    buffers = {"stdout": bytearray(), "stderr": bytearray()}
    deadline = time.monotonic() + timeout
    try:
        while selector.get_map():
            remaining = deadline - time.monotonic()
            if remaining <= 0:
                WORKFLOW_MOD.terminate_tree(process)
                WORKFLOW_MOD.fail(
                    phase,
                    f"process tree exceeded {timeout:g}s deadline",
                    "retry after checking the client/server command for a prompt, hang, or inherited pipe",
                )
            events = selector.select(min(remaining, 0.1))
            for key, _ in events:
                chunk = os.read(key.fd, 65_536)
                if not chunk:
                    selector.unregister(key.fileobj)
                    continue
                target = buffers[key.data]
                target.extend(chunk)
                if len(target) > WORKFLOW_MOD.MAX_BYTES:
                    WORKFLOW_MOD.terminate_tree(process)
                    WORKFLOW_MOD.fail(
                        phase,
                        f"{key.data} exceeded {WORKFLOW_MOD.MAX_BYTES}-byte ceiling",
                        "inspect the command directly; the bounded workflow will not retain unbounded diagnostics",
                    )
    finally:
        selector.close()
    remaining = max(0.0, deadline - time.monotonic())
    try:
        returncode = process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        WORKFLOW_MOD.terminate_tree(process)
        process.wait()
        WORKFLOW_MOD.fail(
            phase,
            f"process tree did not reap within {timeout:g}s deadline",
            "inspect descendant processes spawned by the client command",
        )
    result = WORKFLOW_MOD.CommandResult(
        returncode, bytes(buffers["stdout"]), bytes(buffers["stderr"])
    )
    if returncode not in set(allowed_codes):
        diagnostic = (result.stderr or result.stdout).decode("utf-8", "replace").strip()
        diagnostic = diagnostic[-600:] if diagnostic else "no diagnostic output"
        WORKFLOW_MOD.fail(
            phase,
            f"command exited {returncode}: {diagnostic}",
            "run the named phase directly with the same fresh config and consumer directory",
        )
    return result


class BoundedStdinTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.cwd = Path(self._tmp.name)
        self.pidfile = self.cwd / "child.pid"

    def tearDown(self) -> None:
        _reap_probe(self.pidfile)
        self._tmp.cleanup()

    def _run(self, action: str, stdin: bytes) -> Outcome:
        return _invoke(
            WORKFLOW_MOD.run_bounded,
            _python(_probe_body(action)),
            stdin,
            self.cwd,
            self.pidfile,
        )

    def _mut(self, action: str, stdin: bytes) -> Outcome:
        return _invoke(
            run_bounded_blocking_stdin_before_deadline,
            _python(_probe_body(action)),
            stdin,
            self.cwd,
            self.pidfile,
        )

    def test_non_reader_fails_by_deadline(self) -> None:
        outcome = self._run("non_reader", STDIN_256K)
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("deadline", outcome.reason)
        self.assertLess(outcome.elapsed, SLEEP_S)

    def test_write_first_fails_by_deadline(self) -> None:
        outcome = self._run("write_first", STDIN_256K)
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("deadline", outcome.reason)
        self.assertLess(outcome.elapsed, SLEEP_S)

    def test_reader_succeeds(self) -> None:
        outcome = self._run("reader", STDIN_256K)
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)
        self.assertEqual(outcome.result.stdout, b"got:262144\n")
        self.assertLess(outcome.elapsed, SLEEP_S)

    def test_empty_stdin_still_times_out(self) -> None:
        outcome = self._run("non_reader", b"")
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("deadline", outcome.reason)
        self.assertLess(outcome.elapsed, SLEEP_S)

    def test_early_stdin_close_preserves_success(self) -> None:
        outcome = self._run("early_close", STDIN_256K)
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)

    def test_stdout_over_ceiling_terminates_tree(self) -> None:
        outcome = self._run("flood", b"")
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("ceiling", outcome.reason)
        self.assertIn("stdout", outcome.reason)

    def test_mutation_non_reader_false_green_or_hang(self) -> None:
        outcome = self._mut("non_reader", STDIN_256K)
        beyond_budget = outcome.kind == "hang" or (
            outcome.kind == "ok" and outcome.elapsed > float(DEADLINE_S)
        )
        self.assertTrue(
            beyond_budget,
            f"mutation stayed bounded: kind={outcome.kind} elapsed={outcome.elapsed:.3f} reason={outcome.reason!r}",
        )

    def test_mutation_write_first_false_green_or_hang(self) -> None:
        outcome = self._mut("write_first", STDIN_256K)
        beyond_budget = outcome.kind == "hang" or (
            outcome.kind == "ok" and outcome.elapsed > float(DEADLINE_S)
        )
        self.assertTrue(
            beyond_budget,
            f"mutation stayed bounded: kind={outcome.kind} elapsed={outcome.elapsed:.3f} reason={outcome.reason!r}",
        )

    def test_mutation_empty_control_stays_green(self) -> None:
        outcome = self._mut("true", b"")
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)
        self.assertLess(outcome.elapsed, SLEEP_S)


if __name__ == "__main__":
    unittest.main()
