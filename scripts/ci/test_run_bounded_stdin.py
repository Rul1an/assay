#!/usr/bin/env python3
"""Deadline must supervise stdin delivery for run_bounded."""

from __future__ import annotations

import importlib.util
import os
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


FCNTL_BLOCKER = r"""
import sys
from importlib.util import module_from_spec, spec_from_file_location

class BlockFcntl:
    def find_spec(self, name, path=None, target=None):
        if name == "fcntl":
            raise ImportError("simulated-unavailable fcntl")
        return None

sys.meta_path.insert(0, BlockFcntl())
sys.modules.pop("fcntl", None)
spec = spec_from_file_location("wf_no_fcntl", sys.argv[1])
mod = module_from_spec(spec)
sys.modules[spec.name] = mod
spec.loader.exec_module(mod)
"""


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

    def test_empty_stdin_immediate_exit_succeeds(self) -> None:
        outcome = self._run("true", b"")
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)
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


class FcntlUnavailableTests(unittest.TestCase):
    """Import stays safe when fcntl is missing; run_bounded fails only if stdin needs it."""

    def test_import_succeeds_when_fcntl_unavailable(self) -> None:
        script = FCNTL_BLOCKER + "print('imported', mod.MAX_BYTES)\n"
        result = subprocess.run(
            [sys.executable, "-c", script, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("imported", result.stdout)

    def test_run_bounded_fails_explicitly_when_stdin_needs_fcntl(self) -> None:
        script = (
            FCNTL_BLOCKER
            + """
import tempfile
from pathlib import Path
cwd = Path(tempfile.mkdtemp())
try:
    mod.run_bounded(
        "bounded_stdin",
        [sys.executable, "-c", "raise SystemExit(0)"],
        cwd=cwd,
        env=mod.clean_env(),
        stdin=b"x",
    )
except mod.WorkflowError as error:
    print("reason", error.reason)
    raise SystemExit(0)
print("unexpected success")
raise SystemExit(2)
"""
        )
        result = subprocess.run(
            [sys.executable, "-c", script, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("fcntl", result.stdout)
        self.assertIn("POSIX", result.stdout)

    def test_empty_stdin_does_not_require_fcntl(self) -> None:
        script = (
            FCNTL_BLOCKER
            + """
import tempfile
from pathlib import Path
cwd = Path(tempfile.mkdtemp())
result = mod.run_bounded(
    "bounded_stdin",
    [sys.executable, "-c", "raise SystemExit(0)"],
    cwd=cwd,
    env=mod.clean_env(),
    stdin=b"",
)
print("rc", result.returncode)
"""
        )
        result = subprocess.run(
            [sys.executable, "-c", script, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("rc 0", result.stdout)


if __name__ == "__main__":
    unittest.main()
