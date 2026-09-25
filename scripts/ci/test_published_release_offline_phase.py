#!/usr/bin/env python3
"""Behavioral tests for the published-release offline isolation phase.

The child named ``unshare`` is a stand-in. These tests never create a
network namespace and never change host firewall or VM state.
"""

from __future__ import annotations

import importlib.util
import json
import os
from pathlib import Path
import re
import signal
import socket
import stat
import subprocess
import sys
import tempfile
import textwrap
import threading
import time
import unittest


ROOT = Path(__file__).resolve().parents[2]
HELPER_PATH = ROOT / "scripts/ci/published_release_offline_phase.py"
VERIFIER = [
    "assay",
    "evidence",
    "verify-privileged-mcp-action",
    "bundle.tar.gz",
    "--profile-version",
    "v1",
    "--format",
    "json",
]
DENIAL_RECEIPT = (
    '{"errno":"ENETUNREACH","result":"denied","schema":"assay.offline_probe.v1"}\n'
)
CONNECTED_RECEIPT = (
    '{"errno":"","result":"connected","schema":"assay.offline_probe.v1"}\n'
)


def configured_contract_hook_selector(config_text: str) -> tuple[str, str]:
    """Return the live hook's files and exclude patterns.

    The strings are the configured selector. Callers must not paste a second copy.
    """
    hook_id = "published-release-golden-path-contract"
    lines = config_text.splitlines()
    starts = [index for index, line in enumerate(lines) if line == f"      - id: {hook_id}"]
    if len(starts) != 1:
        raise AssertionError(f"expected one {hook_id} hook, found {len(starts)}")
    block: list[str] = []
    for line in lines[starts[0] + 1 :]:
        if line.startswith("      - id:"):
            break
        block.append(line)
    if not any(
        line.strip() == "entry: bash scripts/ci/test-published-release-golden-path-contract.sh"
        for line in block
    ):
        raise AssertionError("contract hook entry is not the golden-path contract script")
    files = [line.strip() for line in block if line.strip().startswith("files:")]
    excludes = [line.strip() for line in block if line.strip().startswith("exclude:")]
    if len(files) != 1:
        raise AssertionError(f"{hook_id} must have one files selector, found {len(files)}")
    if len(excludes) > 1:
        raise AssertionError(f"{hook_id} has multiple exclude selectors")
    include = files[0].removeprefix("files:").strip()
    if len(include) >= 2 and include[0] == include[-1] and include[0] in {"'", '"'}:
        include = include[1:-1]
    if not include or include[0] in {">", "|"}:
        raise AssertionError(f"{hook_id} files selector is not a single-line pattern")
    exclude = "^$"
    if excludes:
        exclude = excludes[0].removeprefix("exclude:").strip()
    return include, exclude


def contract_hook_selected(include: str, exclude: str, filenames: list[str]) -> bool:
    """Same include/exclude predicate an incremental pre-commit run applies."""
    include_re, exclude_re = re.compile(include), re.compile(exclude)
    return any(include_re.search(name) and not exclude_re.search(name) for name in filenames)


def load_helper():
    spec = importlib.util.spec_from_file_location(
        "published_release_offline_phase", HELPER_PATH
    )
    if spec is None or spec.loader is None:
        raise FileNotFoundError(HELPER_PATH)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class OfflinePhaseTests(unittest.TestCase):
    def setUp(self) -> None:
        self.helper = load_helper()
        self.temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="offline-phase-")))
        self.results = self.temporary / "results"
        self.results.mkdir()
        self.bindir = self.temporary / "bin"
        self.bindir.mkdir()
        self.child_log = self.temporary / "child-argv.jsonl"
        self.child_control = self.temporary / "child-control.json"
        self._write_unshare_standin()
        self._old_path = os.environ.get("PATH")
        os.environ["PATH"] = f"{self.bindir}{os.pathsep}{self._old_path or ''}"

    def tearDown(self) -> None:
        if self._old_path is None:
            os.environ.pop("PATH", None)
        else:
            os.environ["PATH"] = self._old_path

    def _write_unshare_standin(self) -> None:
        script = self.bindir / "unshare"
        script.write_text(
            textwrap.dedent(
                f"""\
                #!/usr/bin/env python3
                import json, sys, time, pathlib
                log = pathlib.Path({str(self.child_log)!r})
                control = json.loads(pathlib.Path({str(self.child_control)!r}).read_text())
                with log.open("a", encoding="utf-8") as handle:
                    handle.write(json.dumps(sys.argv) + "\\n")
                kind = "probe" if "--probe" in sys.argv else "verifier"
                spec = control[kind]
                time.sleep(spec.get("sleep", 0))
                sys.stdout.write(spec.get("stdout", ""))
                sys.stderr.write(spec.get("stderr", ""))
                raise SystemExit(spec["exit"])
                """
            ),
            encoding="utf-8",
        )
        script.chmod(script.stat().st_mode | stat.S_IEXEC)

    def _set_control(self, probe: dict, verifier: dict | None = None) -> None:
        self.child_control.write_text(
            json.dumps(
                {
                    "probe": probe,
                    "verifier": verifier or {"exit": 0, "stdout": "", "stderr": ""},
                }
            ),
            encoding="utf-8",
        )

    def _run(self, timeout: int = 5, probe_executable: str | None = None) -> int:
        return self.helper.run_offline_phase(
            self.results, VERIFIER, timeout, probe_executable
        )

    def _operations(self) -> list[dict]:
        path = self.results / "offline-operations.ndjson"
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]

    def _operation(self, name: str) -> dict:
        matches = [row for row in self._operations() if row.get("name") == name]
        self.assertEqual(len(matches), 1, self._operations())
        return matches[0]

    def _as_isolation_argv(self, argv: list[str]) -> list[str]:
        self.assertEqual(Path(argv[0]).name, "unshare", argv)
        return ["unshare", *argv[1:]]

    def _child_invocations(self) -> list[list[str]]:
        if not self.child_log.exists():
            return []
        return [
            json.loads(line)
            for line in self.child_log.read_text(encoding="utf-8").splitlines()
            if line
        ]

    def _assert_listener_closed(self) -> None:
        port_path = self.results / "offline-listener.port"
        self.assertTrue(port_path.is_file(), "listener never became ready")
        port = int(port_path.read_text(encoding="utf-8"))
        probe = socket.socket()
        try:
            probe.settimeout(0.2)
            with self.assertRaises(OSError):
                probe.connect(("127.0.0.1", port))
        finally:
            probe.close()

    def _assert_verifier_not_invoked(self) -> None:
        self.assertFalse(
            any("verify-privileged-mcp-action" in argv for argv in self._child_invocations()),
            self._child_invocations(),
        )
        self.assertNotIn("verify-produced-bundle-offline", [row.get("name") for row in self._operations()])
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_connected_failure_stops_before_verifier(self) -> None:
        failing = self.temporary / "failing-probe"
        failing.write_text("#!/bin/sh\necho connected-broke >&2\nexit 1\n", encoding="utf-8")
        failing.chmod(0o755)
        self._set_control({"exit": 4, "stdout": DENIAL_RECEIPT, "stderr": ""})
        status = self._run(probe_executable=str(failing))
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("connected-probe")["classification"], "connected-failure")
        self.assertEqual(self._operation("connected-probe")["exit_code"], 1)
        self.assertEqual(self._child_invocations(), [])
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_missing_probe_127_is_not_denial(self) -> None:
        self._set_control({"exit": 0, "stdout": CONNECTED_RECEIPT, "stderr": ""})
        status = self._run(probe_executable=str(self.temporary / "missing-probe"))
        self.assertEqual(status, 127)
        row = self._operation("connected-probe")
        self.assertEqual(row["classification"], "missing-probe")
        self.assertEqual(row["exit_code"], 127)
        self.assertEqual(self._child_invocations(), [])
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_isolate_setup_error_is_not_denial(self) -> None:
        self._set_control(
            {"exit": 1, "stdout": "", "stderr": "unshare: Operation not permitted\n"}
        )
        status = self._run()
        self.assertNotEqual(status, 0)
        row = self._operation("isolated-probe")
        self.assertEqual(row["classification"], "isolate-setup")
        self.assertEqual(row["exit_code"], 1)
        self.assertEqual(row["stderr"], "unshare: Operation not permitted\n")
        self.assertEqual(len(self._child_invocations()), 1)
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_nonzero_without_probe_receipt_is_not_denial(self) -> None:
        self._set_control(
            {"exit": 7, "stdout": "", "stderr": "curl: (7) Failed to connect\n"}
        )
        status = self._run()
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("isolated-probe")["classification"], "isolate-setup")
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_timeout_is_not_denial(self) -> None:
        self._set_control({"exit": 0, "stdout": DENIAL_RECEIPT, "stderr": "", "sleep": 5})
        status = self._run(timeout=1)
        self.assertEqual(status, 124)
        row = self._operation("isolated-probe")
        self.assertEqual(row["classification"], "timeout")
        self.assertEqual(row["exit_code"], 124)
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_unexpected_exit_is_not_denial(self) -> None:
        self._set_control({"exit": 9, "stdout": "not-a-probe-receipt\n", "stderr": "odd\n"})
        status = self._run()
        self.assertNotEqual(status, 0)
        row = self._operation("isolated-probe")
        self.assertEqual(row["classification"], "unexpected-exit")
        self.assertEqual(row["exit_code"], 9)
        self.assertEqual(row["stdout"], "not-a-probe-receipt\n")
        self.assertEqual(row["stderr"], "odd\n")
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_isolated_successful_connect_fails(self) -> None:
        self._set_control({"exit": 0, "stdout": CONNECTED_RECEIPT, "stderr": ""})
        status = self._run()
        self.assertNotEqual(status, 0)
        row = self._operation("isolated-probe")
        self.assertEqual(row["classification"], "isolated-connected")
        self.assertEqual(row["stdout"], CONNECTED_RECEIPT)
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_known_denial_runs_verifier_under_the_same_constructor(self) -> None:
        verifier_stdout = '{"schema":"assay.privileged_mcp_action.verify.report.v0"}\n'
        self._set_control(
            {"exit": 4, "stdout": DENIAL_RECEIPT, "stderr": "denied-stderr\n"},
            {"exit": 0, "stdout": verifier_stdout, "stderr": "verify-stderr\n"},
        )
        status = self._run()
        self.assertEqual(status, 0, self._operations())
        connected = self._operation("connected-probe")
        isolated = self._operation("isolated-probe")
        verified = self._operation("verify-produced-bundle-offline")
        self.assertEqual(connected["classification"], "connected")
        self.assertEqual(connected["exit_code"], 0)
        self.assertEqual(isolated["classification"], "network-denied")
        self.assertEqual(isolated["exit_code"], 4)
        self.assertEqual(isolated["stdout"], DENIAL_RECEIPT)
        self.assertEqual(isolated["stderr"], "denied-stderr\n")
        self.assertEqual(isolated["argv"][:2], ["unshare", "-rn"])
        self.assertEqual(verified["argv"][:2], ["unshare", "-rn"])
        self.assertEqual(isolated["argv"][:2], verified["argv"][:2])
        self.assertEqual(isolated["argv"][2:], connected["argv"])
        self.assertEqual(connected["argv"][0], isolated["argv"][2])
        self.assertEqual(verified["argv"][2:], VERIFIER)
        self.assertEqual(verified["exit_code"], 0)
        self.assertEqual(verified["stdout"], verifier_stdout)
        self.assertEqual(verified["stderr"], "verify-stderr\n")
        self.assertEqual((self.results / "verify-offline.json").read_text(encoding="utf-8"), verifier_stdout)
        self.assertEqual((self.results / "verify-offline.stderr").read_text(encoding="utf-8"), "verify-stderr\n")
        invocations = [self._as_isolation_argv(argv) for argv in self._child_invocations()]
        self.assertEqual(invocations, [isolated["argv"], verified["argv"]])
        self._assert_listener_closed()

    def test_oversized_probe_output_is_not_denial(self) -> None:
        self._set_control({"exit": 4, "stdout": "x" * (self.helper.MAX_CAPTURE_BYTES + 1), "stderr": ""})
        status = self._run()
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("isolated-probe")["classification"], "unexpected-exit")
        self._assert_verifier_not_invoked()
        self._assert_listener_closed()

    def test_probe_handshake_and_closed_port_denial(self) -> None:
        listener = self.helper.LoopbackListener()
        try:
            completed = subprocess.run(
                [sys.executable, str(HELPER_PATH), "--probe", "127.0.0.1", str(listener.port)],
                check=False,
                capture_output=True,
                text=True,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            receipt = json.loads(completed.stdout)
            self.assertEqual(receipt["result"], "connected")
            self.assertEqual(receipt["schema"], "assay.offline_probe.v1")
        finally:
            listener.close()
        refused = socket.socket()
        refused.bind(("127.0.0.1", 0))
        closed_port = refused.getsockname()[1]
        refused.close()
        denied = subprocess.run(
            [sys.executable, str(HELPER_PATH), "--probe", "127.0.0.1", str(closed_port)],
            check=False,
            capture_output=True,
            text=True,
        )
        self.assertEqual(denied.returncode, 4, denied.stderr)
        receipt = json.loads(denied.stdout)
        self.assertEqual(receipt["result"], "denied")
        self.assertEqual(receipt["errno"], "ECONNREFUSED")

    def _script_pids(self, script: Path) -> list[int]:
        token = str(script)
        completed = subprocess.run(
            ["ps", "-axww", "-o", "pid=,command="],
            check=False,
            capture_output=True,
            text=True,
        )
        found: list[int] = []
        for line in completed.stdout.splitlines():
            pid_text, _, command = line.strip().partition(" ")
            if not pid_text.isdigit() or token not in command:
                continue
            pid = int(pid_text)
            if pid not in {0, 1, os.getpid()}:
                found.append(pid)
        return found

    def _kill_script_pids(self, script: Path) -> None:
        for pid in self._script_pids(script):
            try:
                os.kill(pid, signal.SIGKILL)
            except ProcessLookupError:
                pass

    def _pid_runs_script(self, pid: int, script: Path) -> bool:
        return pid in self._script_pids(script)

    def test_timeout_kills_same_group_grandchild(self) -> None:
        """Timeout cleanup must reach a grandchild that stayed in the child group.

        A setsid grandchild leaves that group. That escape is outside this
        helper's bounded contract, and this test does not build one.
        """
        record = self.temporary / "grandchild.txt"
        script = self.temporary / "same-group-grandchild.py"
        script.write_text(
            textwrap.dedent(
                f"""\
                #!/usr/bin/env python3
                import os, signal, time
                from pathlib import Path
                signal.signal(signal.SIGHUP, signal.SIG_IGN)
                record = Path({str(record)!r})
                if os.fork() == 0:
                    signal.signal(signal.SIGHUP, signal.SIG_IGN)
                    devnull = os.open(os.devnull, os.O_RDWR)
                    for fd in (0, 1, 2):
                        os.dup2(devnull, fd)
                    if devnull > 2:
                        os.close(devnull)
                    record.write_text(f"{{os.getpid()}} {{os.getpgrp()}} {{os.getppid()}}\\n")
                    while True:
                        time.sleep(60)
                for _ in range(200):
                    if record.exists() and record.stat().st_size:
                        break
                    time.sleep(0.01)
                else:
                    raise SystemExit("grandchild did not record its process group")
                time.sleep(60)
                """
            ),
            encoding="utf-8",
        )
        script.chmod(0o755)
        outcome: dict[str, object] = {}

        def invoke() -> None:
            try:
                outcome["status"] = self._run(timeout=1, probe_executable=str(script))
            except BaseException as exc:
                outcome["error"] = exc

        worker = threading.Thread(target=invoke, daemon=True)
        worker.start()
        worker.join(5)
        try:
            self.assertFalse(worker.is_alive(), "timeout cleanup did not return")
            self.assertNotIn("error", outcome)
            self.assertEqual(outcome["status"], 124)
            self.assertEqual(self._operation("connected-probe")["classification"], "timeout")
            self._assert_verifier_not_invoked()
            self._assert_listener_closed()
            text = record.read_text(encoding="utf-8").split()
            self.assertEqual(len(text), 3, text)
            pid, group, parent = (int(part) for part in text)
            self.assertEqual(group, parent)
            self.assertNotEqual(pid, group)
            self.assertNotEqual(group, os.getpgrp())
            deadline = time.monotonic() + 2
            while time.monotonic() < deadline and self._pid_runs_script(pid, script):
                time.sleep(0.05)
            self.assertFalse(self._pid_runs_script(pid, script), f"grandchild {pid} survived timeout cleanup")
        finally:
            self._kill_script_pids(script)
            worker.join(2)

    def test_production_form_parses_cwd_timeout_and_verifier(self) -> None:
        parsed = self.helper.parse_phase(["--timeout-seconds", "30", "--", *VERIFIER])
        self.assertEqual(parsed, (Path.cwd(), 30, VERIFIER))

    def test_cli_refuses_results_and_probe_executable(self) -> None:
        previous = Path.cwd()
        os.chdir(self.results)
        try:
            refused = (
                ["--results", str(self.results), "--", *VERIFIER],
                ["--probe-executable", str(self.temporary / "probe"), "--", *VERIFIER],
            )
            for argv in refused:
                flag = argv[0]
                with self.subTest(flag=flag):
                    with self.assertRaises(SystemExit) as caught:
                        self.helper.main(argv)
                    self.assertEqual(
                        str(caught.exception),
                        f"unknown offline phase argument: {flag}",
                    )
        finally:
            os.chdir(previous)
        self.assertEqual(self._operations(), [])

    def test_probe_timeout_is_not_a_denial_receipt(self) -> None:
        held = socket.socket()
        held.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        held.bind(("127.0.0.1", 0))
        held.listen(1)
        port = held.getsockname()[1]
        try:
            completed = subprocess.run(
                [
                    sys.executable,
                    str(HELPER_PATH),
                    "--probe",
                    "127.0.0.1",
                    str(port),
                    "--probe-timeout",
                    "0.2",
                ],
                check=False,
                capture_output=True,
                text=True,
                timeout=2,
            )
        finally:
            held.close()
        self.assertEqual(completed.returncode, 3, completed.stderr)
        receipt = json.loads(completed.stdout)
        self.assertEqual(receipt["result"], "timeout")
        self.assertNotEqual(receipt["result"], "denied")


class ContractHookSelectorTests(unittest.TestCase):
    def test_helper_only_and_test_only_select_the_contract_hook(self) -> None:
        include, exclude = configured_contract_hook_selector(
            (ROOT / ".pre-commit-config.yaml").read_text(encoding="utf-8")
        )
        helper = "scripts/ci/published_release_offline_phase.py"
        tests = "scripts/ci/test_published_release_offline_phase.py"
        self.assertTrue(contract_hook_selected(include, exclude, [helper]), helper)
        self.assertTrue(contract_hook_selected(include, exclude, [tests]), tests)
        unrelated = (
            "README.md",
            "docs/LAUNCH.md",
            "crates/assay-cli/src/main.rs",
            "scripts/ci/published_release_offline_phase.py.bak",
            "scripts/ci/test_published_release_offline_phase.py.bak",
        )
        for path in unrelated:
            self.assertFalse(contract_hook_selected(include, exclude, [path]), path)


if __name__ == "__main__":
    unittest.main()
