#!/usr/bin/env python3
"""Deadline must supervise stdin delivery for run_bounded."""

from __future__ import annotations

import importlib.util
import json
import os
import re
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
SUCCESS_DEADLINE_S = "0.5"
SLEEP_S = 0.35
WALL_S = 1.0
SUBPROCESS_ENV = {**os.environ, "ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS": DEADLINE_S}
PRE_COMMIT_CONFIG = ROOT / ".pre-commit-config.yaml"
TEST_OWNED_STDIN_HOOK_PATHS = {
    PRE_COMMIT_CONFIG.relative_to(ROOT).as_posix(),
    Path(__file__).resolve().relative_to(ROOT).as_posix(),
    Path(__file__)
    .resolve()
    .with_name("claude_plugin_install_workflow.py")
    .relative_to(ROOT)
    .as_posix(),
}


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
        "import json, os, subprocess, sys, time\n"
        "from pathlib import Path\n"
        "Path(os.environ['BOUNDED_STDIN_PIDFILE']).write_text(str(os.getpid()))\n"
        "if os.environ.get('BOUNDED_STDIN_CHILD_RECORD'):\n"
        "    Path(os.environ['BOUNDED_STDIN_CHILD_RECORD']).write_text(\n"
        "        json.dumps({'pid': os.getpid(), 'pgid': os.getpgid(0)})\n"
        "    )\n"
    )
    actions = {
        "non_reader": "time.sleep(0.35)\n",
        "group_descendant": (
            "descendant = subprocess.Popen(\n"
            "    [sys.executable, '-c', 'import time; time.sleep(30)'],\n"
            "    stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,\n"
            ")\n"
            "Path(os.environ['BOUNDED_STDIN_DESCENDANT_RECORD']).write_text(\n"
            "    json.dumps({'pid': descendant.pid, 'pgid': os.getpgid(descendant.pid)})\n"
            ")\n"
            "time.sleep(30)\n"
        ),
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
            "time.sleep(0.08)\n"
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
    timeout_s: str = DEADLINE_S,
    extra_env: dict[str, str] | None = None,
) -> Outcome:
    env = WORKFLOW_MOD.clean_env(
        {"BOUNDED_STDIN_PIDFILE": str(pidfile), "PYTHONUNBUFFERED": "1"}
    )
    if extra_env:
        env.update(extra_env)
    box: dict[str, Any] = {}

    def target() -> None:
        with patch.dict(
            os.environ,
            {"ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS": timeout_s},
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


def _pid_alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    return True


def _wait_process_record(path: Path, timeout: float = 1.0) -> tuple[int, int] | None:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (FileNotFoundError, json.JSONDecodeError):
            time.sleep(0.01)
            continue
        if isinstance(value, dict) and all(
            isinstance(value.get(key), int) and value[key] > 0 for key in ("pid", "pgid")
        ):
            return value["pid"], value["pgid"]
        time.sleep(0.01)
    return None


def _wait_pid_gone(pid: int, timeout: float = 1.0) -> bool:
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if not _pid_alive(pid):
            return True
        time.sleep(0.01)
    return not _pid_alive(pid)


def _reap_process_records(records: list[tuple[int, int]]) -> None:
    for pid, pgid in records:
        if not _pid_alive(pid):
            continue
        for target in (
            lambda: os.killpg(pgid, signal.SIGKILL),
            lambda: os.kill(pid, signal.SIGKILL),
        ):
            try:
                target()
            except (ProcessLookupError, PermissionError):
                pass
    for pid, _pgid in records:
        _wait_pid_gone(pid, timeout=1.0)


def _load_pre_commit_yaml_independently(config: str) -> dict[str, Any]:
    script = (
        'require "json"; require "yaml"; '
        'STDOUT.write(JSON.generate(YAML.safe_load(STDIN.read, aliases: false)))'
    )
    result = subprocess.run(
        ["ruby", "-EUTF-8:UTF-8", "-e", script],
        input=config,
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise AssertionError(result.stderr)
    parsed = json.loads(result.stdout)
    if not isinstance(parsed, dict):
        raise AssertionError("pre-commit config root is not a mapping")
    return parsed


def _test_owned_stdin_selector_paths(config: str) -> set[str]:
    parsed = _load_pre_commit_yaml_independently(config)
    matches: list[dict[str, Any]] = []
    for repo in parsed.get("repos", []):
        if not isinstance(repo, dict) or repo.get("repo") != "local":
            continue
        for hook in repo.get("hooks", []):
            if isinstance(hook, dict) and hook.get("id") == "claude-plugin-run-bounded-stdin":
                matches.append(hook)
    if len(matches) != 1:
        raise AssertionError(f"expected one real bounded-stdin hook, got {len(matches)}")
    pattern = matches[0].get("files")
    if not isinstance(pattern, str):
        raise AssertionError("bounded-stdin hook files selector is not a string")
    compiled = re.compile(pattern)
    tracked = subprocess.run(
        ["git", "ls-files", "-z"],
        cwd=ROOT,
        capture_output=True,
        check=True,
    ).stdout.decode("utf-8").split("\0")
    return {path for path in tracked if path and compiled.search(path)}


def _scan_live_by_proc(proc_root: Path, token: str) -> list[int]:
    leftover: list[int] = []
    for entry in proc_root.iterdir():
        if not entry.name.isdigit():
            continue
        try:
            cmdline = (entry / "cmdline").read_bytes().replace(bytes([0]), b" ").decode(
                "utf-8", "replace"
            )
        except OSError:
            continue
        if token in cmdline:
            leftover.append(int(entry.name))
    return leftover


class BoundedStdinTests(unittest.TestCase):
    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.cwd = Path(self._tmp.name)
        self.pidfile = self.cwd / "child.pid"

    def tearDown(self) -> None:
        _reap_probe(self.pidfile)
        self._tmp.cleanup()

    def _run(self, action: str, stdin: bytes, **kwargs: Any) -> Outcome:
        return _invoke(
            WORKFLOW_MOD.run_bounded,
            _python(_probe_body(action)),
            stdin,
            self.cwd,
            self.pidfile,
            **kwargs,
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
        outcome = self._run("reader", STDIN_256K, timeout_s=SUCCESS_DEADLINE_S)
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)
        self.assertEqual(outcome.result.stdout, b"got:262144\n")

    def test_empty_stdin_still_times_out(self) -> None:
        outcome = self._run("non_reader", b"")
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("deadline", outcome.reason)
        self.assertLess(outcome.elapsed, SLEEP_S)

    def test_empty_stdin_immediate_exit_succeeds(self) -> None:
        outcome = self._run("true", b"", timeout_s=SUCCESS_DEADLINE_S)
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)

    def test_early_stdin_close_preserves_success(self) -> None:
        outcome = self._run("early_close", STDIN_256K, timeout_s=SUCCESS_DEADLINE_S)
        self.assertEqual(outcome.kind, "ok", outcome.reason)
        self.assertEqual(outcome.result.returncode, 0)

    def test_stdout_over_ceiling_terminates_tree(self) -> None:
        outcome = self._run("flood", b"")
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("ceiling", outcome.reason)
        self.assertIn("stdout", outcome.reason)

    def test_deadline_reaps_direct_child_and_descendant(self) -> None:
        child_record_path = self.cwd / "child-record.json"
        descendant_record_path = self.cwd / "descendant-record.json"
        records: list[tuple[int, int]] = []
        try:
            outcome = self._run(
                "group_descendant",
                b"",
                timeout_s="0.4",
                wall=2.0,
                extra_env={
                    "BOUNDED_STDIN_CHILD_RECORD": str(child_record_path),
                    "BOUNDED_STDIN_DESCENDANT_RECORD": str(descendant_record_path),
                },
            )
            self.assertEqual(outcome.kind, "err", outcome.reason)
            self.assertIn("deadline", outcome.reason)

            child = _wait_process_record(child_record_path, timeout=0.2)
            descendant = _wait_process_record(descendant_record_path, timeout=0.2)
            self.assertIsNotNone(child, "missing mandatory direct-child PID/PGID witness")
            self.assertIsNotNone(descendant, "missing mandatory descendant PID/PGID witness")
            assert child is not None
            assert descendant is not None
            records.extend((child, descendant))

            child_pid, child_pgid = child
            descendant_pid, descendant_pgid = descendant
            self.assertEqual(child_pgid, child_pid, "direct child did not lead its new session")
            self.assertEqual(
                descendant_pgid,
                child_pgid,
                "descendant was not in the supervised process group",
            )
            self.assertTrue(_wait_pid_gone(child_pid), f"direct child {child_pid} survived deadline")
            self.assertTrue(
                _wait_pid_gone(descendant_pid),
                f"descendant {descendant_pid} survived deadline",
            )
        finally:
            for path in (child_record_path, descendant_record_path):
                record = _wait_process_record(path, timeout=0.05)
                if record is not None and record not in records:
                    records.append(record)
            _reap_process_records(records)

    def test_exit_7_rejected_preserves_sentinel(self) -> None:
        argv = _python(
            "import sys\nsys.stderr.write('err-sentinel\\n')\nraise SystemExit(7)\n"
        )
        outcome = _invoke(WORKFLOW_MOD.run_bounded, argv, b"", self.cwd, self.pidfile)
        self.assertEqual(outcome.kind, "err", outcome.reason)
        self.assertIn("exited 7", outcome.reason)
        self.assertIn("err-sentinel", outcome.reason)

    def test_exit_7_allowed_preserves_rc_and_stderr(self) -> None:
        argv = _python(
            "import sys\nsys.stderr.write('err-sentinel\\n')\nraise SystemExit(7)\n"
        )
        env = WORKFLOW_MOD.clean_env(
            {"BOUNDED_STDIN_PIDFILE": str(self.pidfile), "PYTHONUNBUFFERED": "1"}
        )
        with patch.dict(
            os.environ,
            {"ASSAY_CLAUDE_WORKFLOW_TIMEOUT_SECONDS": DEADLINE_S},
            clear=False,
        ):
            result = WORKFLOW_MOD.run_bounded(
                "bounded_stdin",
                argv,
                cwd=self.cwd,
                env=env,
                stdin=b"",
                allowed_codes=(7,),
            )
        self.assertEqual(result.returncode, 7)
        self.assertIn(b"err-sentinel", result.stderr)

    def test_mutation_swallow_exit_7_goes_red(self) -> None:
        argv = _python(
            "import sys\nsys.stderr.write('err-sentinel\\n')\nraise SystemExit(7)\n"
        )
        real_fail = WORKFLOW_MOD.fail

        def swallow(phase: str, reason: str, next_step: str) -> None:
            real_fail(phase, "command exited 0: no diagnostic output", next_step)

        with patch.object(WORKFLOW_MOD, "fail", swallow):
            with self.assertRaises(WORKFLOW_MOD.WorkflowError) as mutant:
                WORKFLOW_MOD.run_bounded(
                    "bounded_stdin",
                    argv,
                    cwd=self.cwd,
                    env=WORKFLOW_MOD.clean_env(),
                    stdin=b"",
                )
        self.assertNotIn("exited 7", mutant.exception.reason)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as production:
            WORKFLOW_MOD.run_bounded(
                "bounded_stdin",
                argv,
                cwd=self.cwd,
                env=WORKFLOW_MOD.clean_env(),
                stdin=b"",
            )
        self.assertIn("exited 7", production.exception.reason)
        self.assertIn("err-sentinel", production.exception.reason)

    def test_mutation_replace_stderr_goes_red(self) -> None:
        argv = _python(
            "import sys\nsys.stderr.write('err-sentinel\\n')\nraise SystemExit(7)\n"
        )
        real_fail = WORKFLOW_MOD.fail

        def replace(phase: str, reason: str, next_step: str) -> None:
            real_fail(phase, "command exited 7: replaced-diagnostic", next_step)

        with patch.object(WORKFLOW_MOD, "fail", replace):
            with self.assertRaises(WORKFLOW_MOD.WorkflowError) as mutant:
                WORKFLOW_MOD.run_bounded(
                    "bounded_stdin",
                    argv,
                    cwd=self.cwd,
                    env=WORKFLOW_MOD.clean_env(),
                    stdin=b"",
                )
        self.assertIn("replaced-diagnostic", mutant.exception.reason)
        self.assertNotIn("err-sentinel", mutant.exception.reason)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as production:
            WORKFLOW_MOD.run_bounded(
                "bounded_stdin",
                argv,
                cwd=self.cwd,
                env=WORKFLOW_MOD.clean_env(),
                stdin=b"",
            )
        self.assertIn("err-sentinel", production.exception.reason)
        self.assertNotIn("replaced-diagnostic", production.exception.reason)


class SupervisorPreflightTests(unittest.TestCase):
    def test_import_succeeds_when_fcntl_unavailable(self) -> None:
        script = FCNTL_BLOCKER + "print('imported', mod.MAX_BYTES)\n"
        result = subprocess.run(
            [sys.executable, "-c", script, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
            env=SUBPROCESS_ENV,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertIn("imported", result.stdout)

    def test_fcntl_unavailable_refuses_before_spawn(self) -> None:
        inner = """
import os, tempfile
from pathlib import Path
cwd = Path(tempfile.mkdtemp())
marker = cwd / "started"
child = "from pathlib import Path; Path(%r).write_text('started'); import time; time.sleep(30)" % str(marker)
try:
    mod.run_bounded(
        "bounded_stdin",
        [sys.executable, "-c", child],
        cwd=cwd,
        env=mod.clean_env(),
        stdin=b"x",
    )
    print("unexpected success")
    raise SystemExit(2)
except mod.WorkflowError as error:
    print("reason", error.reason)
import time
deadline = time.monotonic() + 0.25
while time.monotonic() < deadline and not marker.exists():
    time.sleep(0.01)
print("marker", marker.exists())
"""
        result = subprocess.run(
            [sys.executable, "-c", FCNTL_BLOCKER + inner, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
            env=SUBPROCESS_ENV,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("supervisor", result.stdout)
        self.assertIn("fcntl", result.stdout)
        self.assertNotIn("nonblocking stdin", result.stdout)
        self.assertIn("marker False", result.stdout)

    def test_killpg_unavailable_refuses_before_spawn(self) -> None:
        inner = r"""
import os, tempfile, sys
from pathlib import Path
from importlib.util import module_from_spec, spec_from_file_location
spec = spec_from_file_location("wf_no_killpg", sys.argv[1])
mod = module_from_spec(spec)
sys.modules[spec.name] = mod
spec.loader.exec_module(mod)
if hasattr(os, "killpg"):
    del os.killpg
cwd = Path(tempfile.mkdtemp())
marker = cwd / "started"
child = "from pathlib import Path; Path(%r).write_text('started'); import time; time.sleep(30)" % str(marker)
try:
    mod.run_bounded(
        "bounded_stdin",
        [sys.executable, "-c", child],
        cwd=cwd,
        env=mod.clean_env(),
        stdin=b"",
    )
    print("unexpected success")
    raise SystemExit(2)
except Exception as error:
    print("reason", getattr(error, "reason", type(error).__name__ + ": " + str(error)))
import time
deadline = time.monotonic() + 0.25
while time.monotonic() < deadline and not marker.exists():
    time.sleep(0.01)
print("marker", marker.exists())
"""
        result = subprocess.run(
            [sys.executable, "-c", inner, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
            env=SUBPROCESS_ENV,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("supervisor", result.stdout)
        self.assertIn("killpg", result.stdout)
        self.assertIn("marker False", result.stdout)

    def test_mutation_disable_preflight_starts_marker_child(self) -> None:
        inner = """
import os, signal, subprocess, tempfile, time
from pathlib import Path
cwd = Path(tempfile.mkdtemp())
marker = cwd / "started"
orig = getattr(mod, "require_bounded_supervisor", None)
if orig is None:
    print("missing-preflight")
    raise SystemExit(3)
mod.require_bounded_supervisor = lambda phase: None
real_popen = subprocess.Popen
spawned = []
def wrapping_popen(*args, **kwargs):
    process = real_popen(*args, **kwargs)
    spawned.append(process)
    return process
subprocess.Popen = wrapping_popen
try:
    try:
        mod.run_bounded(
            "bounded_stdin",
            ["/bin/sleep", "30"],
            cwd=cwd,
            env=mod.clean_env(),
            stdin=b"x",
        )
    except mod.WorkflowError as error:
        print("reason", error.reason)
finally:
    subprocess.Popen = real_popen
    deadline = time.monotonic() + 1.0
    while time.monotonic() < deadline and not marker.exists():
        time.sleep(0.01)
    print("spawned", len(spawned))
    print("marker", marker.exists())
    for process in spawned:
        try:
            os.killpg(process.pid, signal.SIGKILL)
        except (ProcessLookupError, PermissionError, OSError):
            pass
        try:
            process.wait(timeout=1.0)
        except Exception:
            pass
    mod.require_bounded_supervisor = orig
"""
        result = subprocess.run(
            [sys.executable, "-c", FCNTL_BLOCKER + inner, str(WORKFLOW)],
            capture_output=True,
            text=True,
            check=False,
            env=SUBPROCESS_ENV,
        )
        self.assertEqual(result.returncode, 0, result.stderr + result.stdout)
        self.assertIn("spawned 1", result.stdout)
        self.assertIn("nonblocking stdin", result.stdout)
        self.assertNotIn("supervisor", result.stdout)

    def test_mutation_proc_scan_fails_without_proc(self) -> None:
        missing = Path(tempfile.mkdtemp()) / "no-proc"
        with self.assertRaises(FileNotFoundError):
            _scan_live_by_proc(missing, "bounded-fcntl-reap-probe")


class RequiredHookTableTests(unittest.TestCase):
    def test_required_hooks_are_pinned(self) -> None:
        WORKFLOW_MOD.assert_required_claude_plugin_hooks()

    def _config(self) -> str:
        return (ROOT / ".pre-commit-config.yaml").read_text(encoding="utf-8")

    def _assert_test_owned_stdin_selector(self, config: str) -> None:
        self.assertEqual(
            _test_owned_stdin_selector_paths(config),
            TEST_OWNED_STDIN_HOOK_PATHS,
        )

    def _selector_mutation(self, old: str, new: str) -> str:
        config = self._config()
        block = WORKFLOW_MOD._hook_block_by_id(config, "claude-plugin-run-bounded-stdin")
        self.assertEqual(block.count(old), 1, f"selector fragment count for {old!r}")
        return config.replace(block, block.replace(old, new, 1), 1)

    def test_stdin_files_selector_has_exact_test_owned_paths(self) -> None:
        self._assert_test_owned_stdin_selector(self._config())

    def test_mutation_stdin_selector_without_workflow_path_goes_red(self) -> None:
        config = self._selector_mutation(
            "(claude_plugin_install_workflow|test_run_bounded_stdin)",
            "(test_run_bounded_stdin)",
        )
        with self.assertRaises(AssertionError):
            self._assert_test_owned_stdin_selector(config)

    def test_mutation_stdin_selector_without_test_path_goes_red(self) -> None:
        config = self._selector_mutation(
            "(claude_plugin_install_workflow|test_run_bounded_stdin)",
            "(claude_plugin_install_workflow)",
        )
        with self.assertRaises(AssertionError):
            self._assert_test_owned_stdin_selector(config)

    def test_mutation_stdin_selector_without_config_path_goes_red(self) -> None:
        config = self._selector_mutation(
            "|\\.pre-commit-config\\.yaml",
            "",
        )
        with self.assertRaises(AssertionError):
            self._assert_test_owned_stdin_selector(config)

    def test_mutation_delete_stdin_hook_goes_red(self) -> None:
        config = self._config().replace("claude-plugin-run-bounded-stdin", "claude-plugin-run-bounded-gone")
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-run-bounded-stdin", str(raised.exception.reason))

    def test_mutation_retarget_stdin_hook_goes_red(self) -> None:
        config = self._config().replace(
            "python3 scripts/ci/test_run_bounded_stdin.py",
            "python3 scripts/ci/test_claude_plugin_install_import.py",
        )
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-run-bounded-stdin", str(raised.exception.reason))

    def test_mutation_delete_self_test_hook_goes_red(self) -> None:
        config = self._config().replace(
            "claude-plugin-install-workflow-self-test",
            "claude-plugin-install-workflow-gone",
        )
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-install-workflow-self-test", str(raised.exception.reason))

    def test_mutation_retarget_self_test_hook_goes_red(self) -> None:
        config = self._config().replace(
            "bash scripts/ci/test-claude-plugin-install.sh --self-test",
            "bash scripts/ci/test-claude-plugin-install.sh --verify",
        )
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-install-workflow-self-test", str(raised.exception.reason))

    def _comment_out_real_id_line(self, hook_id: str) -> str:
        target = f"- id: {hook_id}"
        found = 0
        lines: list[str] = []
        for line in self._config().splitlines(keepends=True):
            stripped = line.lstrip(" \t")
            indent = line[: len(line) - len(stripped)]
            body = stripped.splitlines()[0]
            if body == target:
                newline = line[len(indent) + len(body) :]
                lines.append(f"{indent}# {target}{newline}")
                found += 1
            else:
                lines.append(line)
        if found != 1:
            self.fail(f"expected one real {target!r} line, got {found}")
        return "".join(lines)

    def test_mutation_comment_out_stdin_hook_id_goes_red(self) -> None:
        config = self._comment_out_real_id_line("claude-plugin-run-bounded-stdin")
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-run-bounded-stdin", raised.exception.reason)

    def test_mutation_comment_out_self_test_hook_id_goes_red(self) -> None:
        config = self._comment_out_real_id_line("claude-plugin-install-workflow-self-test")
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-install-workflow-self-test", raised.exception.reason)

    def test_commented_duplicate_id_does_not_replace_real_item(self) -> None:
        hook_id = "claude-plugin-run-bounded-stdin"
        real = f"      - id: {hook_id}"
        config = self._config().replace(real, f"      # - id: {hook_id}\n{real}", 1)
        WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)

    def _scalar_decoy(self) -> str:
        config = self._config()
        blocks: list[str] = []
        for hook_id, _entry in WORKFLOW_MOD.REQUIRED_CLAUDE_PLUGIN_HOOKS:
            block = WORKFLOW_MOD._hook_block_by_id(config, hook_id)
            blocks.append(block)
            config = config.replace(block, "", 1)
        body = "\n".join(blocks)
        indented = "\n".join("    " + line for line in body.splitlines())
        return config.rstrip() + "\n\ndecoy: |\n" + indented + "\n"

    def _ruby_safe_load_ok(self, config: str) -> None:
        script = (
            "require \"yaml\"; YAML.safe_load(STDIN.read, aliases: false); "
            "STDOUT.write(\"ok\")"
        )
        result = subprocess.run(
            ["ruby", "-EUTF-8:UTF-8", "-e", script],
            input=config,
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "ok")

    def test_mutation_scalar_decoy_goes_red(self) -> None:
        config = self._scalar_decoy()
        self._ruby_safe_load_ok(config)
        self.assertIn("- id: claude-plugin-install-workflow-self-test", config)
        self.assertIn("- id: claude-plugin-run-bounded-stdin", config)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-install-workflow-self-test", raised.exception.reason)

    def test_mutation_duplicate_real_hook_goes_red(self) -> None:
        hook_id = "claude-plugin-run-bounded-stdin"
        block = WORKFLOW_MOD._hook_block_by_id(self._config(), hook_id)
        config = self._config().replace(block, block + block, 1)
        self._ruby_safe_load_ok(config)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn(hook_id, raised.exception.reason)

    def test_missing_yaml_parser_fails_closed(self) -> None:
        with patch.object(WORKFLOW_MOD.subprocess, "run", side_effect=FileNotFoundError):
            with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
                WORKFLOW_MOD.assert_required_claude_plugin_hooks()
        self.assertIn("ruby", raised.exception.reason)

    def test_invalid_yaml_fails_closed(self) -> None:
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks("repos: [\n")
        self.assertIn("YAML", raised.exception.reason)

    def _dump_yaml(self, parsed: object) -> str:
        script = (
            "require \"json\"; require \"yaml\"; "
            "STDOUT.write(YAML.dump(JSON.parse(STDIN.read)))"
        )
        result = subprocess.run(
            ["ruby", "-EUTF-8:UTF-8", "-e", script],
            input=json.dumps(parsed),
            capture_output=True,
            text=True,
            check=False,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        return result.stdout

    def _mutate_required_hooks(self, mutator: Callable[[str, dict[str, Any], dict[str, Any]], None]) -> str:
        parsed = WORKFLOW_MOD.load_pre_commit_yaml(self._config())
        required = {hook_id for hook_id, _entry in WORKFLOW_MOD.REQUIRED_CLAUDE_PLUGIN_HOOKS}
        if not isinstance(parsed, dict) or not isinstance(parsed.get("repos"), list):
            self.fail("pre-commit config root is not a mapping with repos")
        for repo in parsed["repos"]:
            if not isinstance(repo, dict) or not isinstance(repo.get("hooks"), list):
                continue
            for hook in repo["hooks"]:
                if isinstance(hook, dict) and hook.get("id") in required:
                    mutator(str(hook["id"]), repo, hook)
        return self._dump_yaml(parsed)

    def test_mutation_required_hooks_on_remote_repo_goes_red(self) -> None:
        parsed = WORKFLOW_MOD.load_pre_commit_yaml(self._config())
        required = {hook_id for hook_id, _entry in WORKFLOW_MOD.REQUIRED_CLAUDE_PLUGIN_HOOKS}
        moved: list[dict[str, Any]] = []
        assert isinstance(parsed, dict)
        for repo in parsed["repos"]:
            if not isinstance(repo, dict) or repo.get("repo") != "local":
                continue
            keep: list[Any] = []
            for hook in repo.get("hooks") or []:
                if isinstance(hook, dict) and hook.get("id") in required:
                    moved.append(hook)
                else:
                    keep.append(hook)
            repo["hooks"] = keep
        parsed["repos"].append(
            {
                "repo": "https://github.com/pre-commit/pre-commit-hooks",
                "rev": "v6.0.0",
                "hooks": moved,
            }
        )
        config = self._dump_yaml(parsed)
        self._ruby_safe_load_ok(config)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("local", raised.exception.reason)

    def test_mutation_self_test_manual_stage_goes_red(self) -> None:
        def mutator(hook_id: str, _repo: dict[str, Any], hook: dict[str, Any]) -> None:
            if hook_id == "claude-plugin-install-workflow-self-test":
                hook["stages"] = ["manual"]
        config = self._mutate_required_hooks(mutator)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-install-workflow-self-test", raised.exception.reason)
        self.assertIn("pre-push", raised.exception.reason)

    def test_mutation_stdin_manual_stage_goes_red(self) -> None:
        def mutator(hook_id: str, _repo: dict[str, Any], hook: dict[str, Any]) -> None:
            if hook_id == "claude-plugin-run-bounded-stdin":
                hook["stages"] = ["manual"]
        config = self._mutate_required_hooks(mutator)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-run-bounded-stdin", raised.exception.reason)
        self.assertIn("pre-commit", raised.exception.reason)

    def test_mutation_stdin_empty_files_selector_goes_red(self) -> None:
        def mutator(hook_id: str, _repo: dict[str, Any], hook: dict[str, Any]) -> None:
            if hook_id == "claude-plugin-run-bounded-stdin":
                hook["files"] = "^$"
        config = self._mutate_required_hooks(mutator)
        with self.assertRaises(WORKFLOW_MOD.WorkflowError) as raised:
            WORKFLOW_MOD.assert_required_claude_plugin_hooks(config)
        self.assertIn("claude-plugin-run-bounded-stdin", raised.exception.reason)

    def test_unrelated_local_hook_moved_remote_stays_green(self) -> None:
        parsed = WORKFLOW_MOD.load_pre_commit_yaml(self._config())
        assert isinstance(parsed, dict)
        moved: list[dict[str, Any]] = []
        for repo in parsed["repos"]:
            if not isinstance(repo, dict) or repo.get("repo") != "local":
                continue
            keep: list[Any] = []
            for hook in repo.get("hooks") or []:
                if isinstance(hook, dict) and hook.get("id") == "claude-plugin-install-import-safe":
                    moved.append(hook)
                else:
                    keep.append(hook)
            repo["hooks"] = keep
        parsed["repos"].append(
            {
                "repo": "https://github.com/pre-commit/pre-commit-hooks",
                "rev": "v6.0.0",
                "hooks": moved,
            }
        )
        WORKFLOW_MOD.assert_required_claude_plugin_hooks(self._dump_yaml(parsed))


if __name__ == "__main__":
    unittest.main()
