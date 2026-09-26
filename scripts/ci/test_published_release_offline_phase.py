#!/usr/bin/env python3
"""Behavioral tests for the published-release offline isolation phase.

The child named ``unshare`` is a stand-in. These tests never create a
network namespace and never change host firewall or VM state.
"""

from __future__ import annotations

import errno
import importlib.util
import json
import os
from contextlib import nullcontext
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
from unittest import mock


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
        # Pre-existing Linux constructor. Pinning the platform keeps that control
        # on unshare when isolation_argv also grows a Darwin body.
        self._linux_platform = mock.patch.object(self.helper.sys, "platform", "linux")
        self._linux_platform.start()

    def tearDown(self) -> None:
        self._linux_platform.stop()
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

    def test_accept_timeout_oserror_does_not_stop_the_listener(self) -> None:
        """One accept timeout must not retire the listener.

        Python 3.9 raises socket.timeout, an OSError that is not TimeoutError,
        from a listening socket with a timeout. That split is what Xcode's
        python3 hits. Where this interpreter aliases the two, the stand-in is
        that same split: an OSError the TimeoutError handler does not catch.
        """
        if socket.timeout is TimeoutError:

            class LegacySocketTimeout(OSError):
                pass

            timeout_type: type[BaseException] = LegacySocketTimeout
            patched_timeout = mock.patch.object(
                self.helper.socket, "timeout", LegacySocketTimeout
            )
        else:
            timeout_type = socket.timeout
            patched_timeout = nullcontext()

        real_accept = socket.socket.accept
        fired = {"count": 0}

        def accept(sock: socket.socket):
            fired["count"] += 1
            if fired["count"] == 1:
                raise timeout_type("timed out")
            return real_accept(sock)

        with patched_timeout, mock.patch.object(socket.socket, "accept", accept):
            listener = self.helper.LoopbackListener()
            try:
                deadline = time.monotonic() + 2
                while fired["count"] < 1 and time.monotonic() < deadline:
                    time.sleep(0.01)
                self.assertGreaterEqual(fired["count"], 1, "listener never accepted")
                self.assertTrue(listener._thread.is_alive(), "accept timeout ended the listener")
                completed = subprocess.run(
                    [sys.executable, str(HELPER_PATH), "--probe", "127.0.0.1", str(listener.port)],
                    check=False,
                    capture_output=True,
                    text=True,
                    timeout=3,
                )
                self.assertEqual(completed.returncode, 0, completed.stdout + completed.stderr)
                receipt = json.loads(completed.stdout)
                self.assertEqual(receipt["result"], "connected")
                self.assertEqual(receipt["errno"], "")
                self.assertGreaterEqual(fired["count"], 2, "listener did not accept again")
                self.assertTrue(listener._thread.is_alive())
            finally:
                listener.close()

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


class DarwinIsolationTests(unittest.TestCase):
    """Darwin sandbox-exec body. Linux unshare tests above stay the control."""

    def setUp(self) -> None:
        self.helper = load_helper()
        self.temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="darwin-offline-")))
        self.results = self.temporary / "results"
        self.results.mkdir()
        self._darwin = mock.patch.object(self.helper.sys, "platform", "darwin")
        self._darwin.start()

    def tearDown(self) -> None:
        self._darwin.stop()

    def test_restrictive_profile_is_sandbox_exec_and_denies_network(self) -> None:
        argv = self.helper.isolation_argv(["probe-token"])
        self.assertEqual(argv[:2], ["/usr/bin/sandbox-exec", "-p"])
        self.assertIn("(deny network*)", argv[2])
        self.assertNotIn("(allow network*)", argv[2])
        self.assertEqual(argv[3:], ["probe-token"])
        permissive = argv[2].replace("(deny network*)", "(allow network*)")
        self.assertEqual(permissive, self.helper.DARWIN_PERMISSIVE_PROFILE)
        self.assertNotIn("(deny network*)", permissive)
        self.assertNotIn("EPERM", self.helper.DENIAL_ERRNO_NAMES)

    def test_eperm_under_linux_constructor_is_refused_control(self) -> None:
        """Pre-existing Linux allow-list. EPERM must not join it."""
        receipt = (
            '{"errno":"EPERM","result":"denied","schema":"assay.offline_probe.v1"}\n'
        ).encode()
        self.assertNotIn(errno.EPERM, self.helper.DENIAL_ERRNOS)
        classified = self.helper.classify_isolated(4, receipt, b"", ["unshare", "-rn", "probe"])
        self.assertNotEqual(classified, "network-denied")

    def test_eperm_under_darwin_allow_list_is_denial(self) -> None:
        receipt = (
            '{"errno":"EPERM","result":"denied","schema":"assay.offline_probe.v1"}\n'
        ).encode()
        observed = self.helper.classify_probe_oserror(OSError(errno.EPERM, "Operation not permitted"))
        self.assertEqual(observed, ("denied", "EPERM", 4))
        argv = self.helper.isolation_argv(["probe-token"])
        classified = self.helper.classify_isolated(4, receipt, b"", argv)
        self.assertEqual(classified, "network-denied")
        self.assertEqual(self.helper.DARWIN_DENIAL_ERRNO_NAMES, frozenset({"EPERM"}))

    def test_missing_receipt_is_setup_not_denial_control(self) -> None:
        """Pre-existing guard: no probe receipt is isolate-setup, not network denial."""
        classified = self.helper.classify_isolated(7, b"", b"sandbox-exec: failed\n")
        self.assertEqual(classified, "isolate-setup")
        self.assertNotEqual(classified, "network-denied")

    def _profile_standin(self) -> Path:
        script = self.temporary / "sandbox-exec"
        script.write_text(
            textwrap.dedent(
                """\
                #!/usr/bin/env python3
                import sys
                joined = " ".join(sys.argv)
                if "--probe" not in sys.argv:
                    sys.stdout.write('{"schema":"assay.privileged_mcp_action.verify.report.v0"}\\n')
                    raise SystemExit(0)
                if "(deny network*)" in joined:
                    sys.stdout.write(
                        '{"errno":"EPERM","result":"denied","schema":"assay.offline_probe.v1"}\\n'
                    )
                    raise SystemExit(4)
                sys.stdout.write(
                    '{"errno":"","result":"connected","schema":"assay.offline_probe.v1"}\\n'
                )
                raise SystemExit(0)
                """
            ),
            encoding="utf-8",
        )
        script.chmod(0o755)
        return script

    def test_removing_network_rule_turns_denial_control_red(self) -> None:
        standin = self._profile_standin()
        self.helper.SANDBOX_EXEC = str(standin)
        denied = self.helper.run_offline_phase(self.results, VERIFIER, 5, None)
        self.assertEqual(denied, 0, (self.results / "offline-operations.ndjson").read_text())
        operations = [
            json.loads(line)
            for line in (self.results / "offline-operations.ndjson").read_text().splitlines()
            if line
        ]
        isolated = next(row for row in operations if row["name"] == "isolated-probe")
        verified = next(row for row in operations if row["name"] == "verify-produced-bundle-offline")
        self.assertEqual(isolated["classification"], "network-denied")
        self.assertIn("(deny network*)", isolated["argv"][2])
        self.assertEqual(verified["argv"][:2], isolated["argv"][:2])
        self.assertIn("(deny network*)", verified["argv"][2])
        permissive = self.helper.DARWIN_RESTRICTIVE_PROFILE.replace("(deny network*)", "(allow network*)")
        self.helper.DARWIN_RESTRICTIVE_PROFILE = permissive
        removed = self.temporary / "removed"
        removed.mkdir()
        status = self.helper.run_offline_phase(removed, VERIFIER, 5, None)
        self.assertNotEqual(status, 0)
        removed_ops = [
            json.loads(line)
            for line in (removed / "offline-operations.ndjson").read_text().splitlines()
            if line
        ]
        removed_isolated = next(row for row in removed_ops if row["name"] == "isolated-probe")
        self.assertEqual(removed_isolated["classification"], "isolated-connected")
        self.assertNotIn("(deny network*)", removed_isolated["argv"][2])
        self.assertFalse((removed / "verify-offline.json").exists())
        self.assertFalse((removed / "offline-cleanup.json").exists())


HOST_TOKEN = {"is_app_container": False, "sid": None, "capabilities": []}
PROFILE_SID = "S-1-15-2-1234"
ZERO_TOKEN = {"is_app_container": True, "sid": PROFILE_SID, "capabilities": []}
PERMISSIVE_TOKEN = {
    "is_app_container": True,
    "sid": PROFILE_SID,
    "capabilities": ["S-1-15-3-1"],
}
WINDOWS_TIMEOUT_RECEIPT = (
    '{"errno":"ETIMEDOUT","result":"timeout","schema":"assay.offline_probe.v1"}\n'
)
WINDOWS_DENIAL_RECEIPT = (
    '{"errno":"EACCES","result":"denied","schema":"assay.offline_probe.v1","winerror":10013}\n'
)


class ScriptedLauncher:
    """Records the calls the phase makes. It does not create an AppContainer."""

    def __init__(self, outcomes: list[dict]) -> None:
        self.outcomes = list(outcomes)
        self.calls: list[tuple] = []
        self.cleanup_result = {
            "status": "clean",
            "steps": {
                "aces_revoked": True,
                "job_closed": True,
                "no_container_sid_ace": True,
                "profile_delete_attempted": True,
                "storage_absent": True,
            },
        }

    def prepare(self, grant_paths: list[str]) -> dict:
        self.calls.append(("prepare", list(grant_paths)))
        return {
            "profile": {"folder": "C:\\AppContainers\\probe", "name": "a1abcd1234", "sid": PROFILE_SID},
            "grants": [{"path": path, "spec": "grant"} for path in grant_paths],
        }

    def launch(self, argv: list[str], env: dict, timeout: int, capabilities: list[str] | None) -> dict:
        self.calls.append(("launch", list(argv), None if capabilities is None else list(capabilities)))
        if not self.outcomes:
            raise AssertionError(f"unexpected launch: {argv}")
        spec = self.outcomes.pop(0)
        if spec.get("accept"):
            port = int(argv[argv.index("--probe") + 2])
            with socket.create_connection(("127.0.0.1", port), timeout=2) as probe:
                probe.recv(5)
        stdout = spec.get("stdout", CONNECTED_RECEIPT.encode())
        return {
            "create_process": spec.get("create_process", True),
            "exit": spec.get("exit", 0),
            "job_processes": spec.get("job_processes", [{"message": "NEW_PROCESS", "pid": spec.get("pid", 1000)}]),
            "job_total_processes": spec.get("job_total_processes", 1),
            "last_error": spec.get("last_error"),
            "pid": spec.get("pid", 1000),
            "stderr": spec.get("stderr", b""),
            "stdout": stdout,
            "token": spec.get("token", HOST_TOKEN),
            "truncated": False,
            "wait_result": spec.get("wait_result", "exited"),
        }

    def terminate_job(self, result: dict) -> None:
        self.calls.append(("TerminateJobObject", result.get("pid")))

    def cleanup(self, state: dict) -> dict:
        profile = state.get("profile") or {}
        self.calls.append(("cleanup", profile.get("sid"), [item.get("path") for item in state.get("grants") or []]))
        return self.cleanup_result


class WindowsIsolationTests(unittest.TestCase):
    """Windows AppContainer body. Linux and Darwin tests above stay the control."""

    def setUp(self) -> None:
        self.helper = load_helper()
        self.temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="windows-offline-")))
        self.results = self.temporary / "results"
        self.results.mkdir()
        self.bin_dir = self.temporary / "install" / "bin"
        self.bin_dir.mkdir(parents=True)
        self.verifier = [str(self.bin_dir / "assay.exe"), *VERIFIER[1:]]
        self._windows = mock.patch.object(self.helper.sys, "platform", "win32")
        self._windows.start()
        real_getaddrinfo = socket.getaddrinfo

        def resolve(host, port, *args, **kwargs):
            if host == "github.com":
                return [(socket.AF_INET, socket.SOCK_STREAM, 6, "", ("140.82.114.4", 443))]
            return real_getaddrinfo(host, port, *args, **kwargs)

        self._resolve = mock.patch.object(self.helper.socket, "getaddrinfo", resolve)
        self._resolve.start()

    def tearDown(self) -> None:
        self._resolve.stop()
        self._windows.stop()

    def _run(self, launcher: ScriptedLauncher, timeout: int = 5) -> int:
        return self.helper.run_offline_phase(self.results, self.verifier, timeout, None, launcher)

    def _operations(self) -> list[dict]:
        path = self.results / "offline-operations.ndjson"
        if not path.exists():
            return []
        return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]

    def _operation(self, name: str) -> dict:
        matches = [row for row in self._operations() if row.get("name") == name]
        self.assertEqual(len(matches), 1, self._operations())
        return matches[0]

    def _happy_outcomes(self) -> list[dict]:
        denied = {"exit": 4, "stdout": WINDOWS_DENIAL_RECEIPT.encode(), "token": ZERO_TOKEN}
        return [
            {"accept": True, "stdout": CONNECTED_RECEIPT.encode(), "token": HOST_TOKEN},
            {"stdout": CONNECTED_RECEIPT.encode(), "token": HOST_TOKEN},
            {"stdout": CONNECTED_RECEIPT.encode(), "token": PERMISSIVE_TOKEN},
            {"exit": 3, "stdout": WINDOWS_TIMEOUT_RECEIPT.encode(), "token": ZERO_TOKEN},
            denied,
            {"exit": 0, "stdout": b'{"schema":"assay.privileged_mcp_action.verify.report.v0"}\n', "token": ZERO_TOKEN},
        ]

    def test_windows_constructor_is_appcontainer_descriptor_not_argv(self) -> None:
        launcher = ScriptedLauncher(self._happy_outcomes())
        status = self._run(launcher)
        self.assertEqual(status, 0, self._operations())
        isolated = self._operation("isolated-probe")
        verified = self._operation("verify-produced-bundle-offline")
        self.assertEqual(isolated["isolation"]["kind"], "appcontainer")
        self.assertEqual(isolated["isolation"]["profile_sid"], PROFILE_SID)
        self.assertEqual(isolated["isolation"]["capabilities"], [])
        self.assertEqual(verified["isolation"], isolated["isolation"])
        self.assertEqual(isolated["external_address"], "140.82.114.4")
        for row in self._operations():
            self.assertNotEqual(row["argv"][:2], ["unshare", "-rn"])
            self.assertNotEqual(row["argv"][:1], ["/usr/bin/sandbox-exec"])
        self.assertNotIn("pid", json.loads(isolated["stdout"]))

    def test_eacces_under_windows_constructor_is_denial(self) -> None:
        receipt = WINDOWS_DENIAL_RECEIPT.encode()
        descriptor = {"capabilities": [], "kind": "appcontainer", "profile_sid": PROFILE_SID}
        classified = self.helper.classify_isolated(4, receipt, b"", descriptor)
        self.assertEqual(classified, "network-denied")
        self.assertEqual(self.helper.WINDOWS_DENIAL, frozenset({("EACCES", 10013)}))

    def test_eacces_under_linux_is_refused_control(self) -> None:
        receipt = WINDOWS_DENIAL_RECEIPT.encode()
        self.assertNotIn("EACCES", self.helper.DENIAL_ERRNO_NAMES)
        classified = self.helper.classify_isolated(4, receipt, b"", ["unshare", "-rn", "probe"])
        self.assertNotEqual(classified, "network-denied")

    def test_eacces_under_darwin_is_refused_control(self) -> None:
        receipt = WINDOWS_DENIAL_RECEIPT.encode()
        argv = ["/usr/bin/sandbox-exec", "-p", self.helper.DARWIN_RESTRICTIVE_PROFILE, "probe"]
        classified = self.helper.classify_isolated(4, receipt, b"", argv)
        self.assertNotEqual(classified, "network-denied")

    def test_loopback_timeout_with_zero_accepts_is_denial_only_on_windows(self) -> None:
        receipt = WINDOWS_TIMEOUT_RECEIPT.encode()
        descriptor = {"capabilities": [], "kind": "appcontainer", "profile_sid": PROFILE_SID}
        denied = self.helper.classify_isolated(3, receipt, b"", descriptor, listener_accepts=0)
        self.assertEqual(denied, "network-denied")
        linux = self.helper.classify_isolated(
            3, receipt, b"", ["unshare", "-rn", "probe"], listener_accepts=0
        )
        self.assertEqual(linux, "timeout")
        self.assertNotEqual(linux, "network-denied")

    def test_loopback_timeout_with_an_accept_is_not_denial(self) -> None:
        receipt = WINDOWS_TIMEOUT_RECEIPT.encode()
        descriptor = {"capabilities": [], "kind": "appcontainer", "profile_sid": PROFILE_SID}
        classified = self.helper.classify_isolated(3, receipt, b"", descriptor, listener_accepts=1)
        self.assertEqual(classified, "unexpected-exit")
        self.assertNotEqual(classified, "network-denied")

    def test_permissive_arm_must_connect_before_denial_counts(self) -> None:
        launcher = ScriptedLauncher(
            [
                {"accept": True, "stdout": CONNECTED_RECEIPT.encode(), "token": HOST_TOKEN},
                {"stdout": CONNECTED_RECEIPT.encode(), "token": HOST_TOKEN},
                {"exit": 5, "stdout": b'{"errno":"","result":"error","schema":"assay.offline_probe.v1"}\n', "token": PERMISSIVE_TOKEN},
            ]
        )
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("isolated-permissive-probe")["classification"], "permissive-control-failed")
        self.assertNotIn("verify-produced-bundle-offline", [row["name"] for row in self._operations()])
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_removing_capability_difference_turns_control_red(self) -> None:
        outcomes = self._happy_outcomes()
        outcomes[3] = {"accept": True, "exit": 0, "stdout": CONNECTED_RECEIPT.encode(), "token": ZERO_TOKEN}
        launcher = ScriptedLauncher(outcomes)
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("isolated-probe")["classification"], "isolated-connected")
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_token_readback_mismatch_is_setup(self) -> None:
        wrong = {"is_app_container": True, "sid": "S-1-15-2-9999", "capabilities": []}
        outcomes = self._happy_outcomes()
        outcomes[3] = {"exit": 4, "stdout": WINDOWS_DENIAL_RECEIPT.encode(), "token": wrong}
        launcher = ScriptedLauncher(outcomes)
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        row = self._operation("isolated-probe")
        self.assertEqual(row["classification"], "isolate-setup")
        self.assertNotEqual(row["classification"], "network-denied")
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_launch_failure_records_getlasterror_as_setup(self) -> None:
        launcher = ScriptedLauncher(
            [{"create_process": False, "exit": 1, "last_error": 5, "stdout": WINDOWS_DENIAL_RECEIPT.encode(), "token": HOST_TOKEN}]
        )
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        row = self._operation("connected-probe")
        self.assertEqual(row["classification"], "isolate-setup")
        self.assertEqual(row["last_error"], 5)
        self.assertNotEqual(row["classification"], "network-denied")

    def test_timeout_terminates_job_not_just_process(self) -> None:
        launcher = ScriptedLauncher(
            [{"exit": 259, "pid": 4242, "stdout": b"", "token": HOST_TOKEN, "wait_result": "timeout"}]
        )
        status = self._run(launcher)
        self.assertEqual(status, 124)
        self.assertEqual(self._operation("connected-probe")["classification"], "timeout")
        self.assertIn(("TerminateJobObject", 4242), launcher.calls)
        self.assertNotIn("verify-produced-bundle-offline", [row["name"] for row in self._operations()])

    def test_verifier_runs_with_same_sid_and_empty_capabilities(self) -> None:
        launcher = ScriptedLauncher(self._happy_outcomes())
        status = self._run(launcher)
        self.assertEqual(status, 0, self._operations())
        isolated = self._operation("isolated-probe")
        verified = self._operation("verify-produced-bundle-offline")
        self.assertEqual(verified["isolation"], isolated["isolation"])
        self.assertEqual(verified["isolation"]["capabilities"], self.helper.WINDOWS_ZERO_CAPABILITIES)
        launches = [call for call in launcher.calls if call[0] == "launch"]
        self.assertEqual(launches[3][2], [])
        self.assertEqual(launches[4][2], [])
        self.assertEqual(launches[5][2], [])
        self.assertEqual(launches[5][1][0], self.verifier[0])

    def test_missing_receipt_is_not_denial(self) -> None:
        outcomes = self._happy_outcomes()
        outcomes[3] = {"exit": 1, "stdout": b"", "token": ZERO_TOKEN}
        launcher = ScriptedLauncher(outcomes)
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        self.assertEqual(self._operation("isolated-probe")["classification"], "isolate-setup")
        self.assertNotEqual(self._operation("isolated-probe")["classification"], "network-denied")
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_cleanup_runs_on_every_path_and_dirty_fails_phase(self) -> None:
        failed = ScriptedLauncher(
            [{"create_process": False, "exit": 1, "last_error": 2, "stdout": b"", "token": HOST_TOKEN}]
        )
        status = self._run(failed)
        self.assertNotEqual(status, 0)
        self.assertTrue(any(call[0] == "cleanup" for call in failed.calls))
        self.assertEqual(failed.calls[-1][1], PROFILE_SID)
        dirty = ScriptedLauncher(self._happy_outcomes())
        dirty.cleanup_result = {**dirty.cleanup_result, "status": "dirty"}
        dirty_results = self.temporary / "dirty"
        dirty_results.mkdir()
        status = self.helper.run_offline_phase(dirty_results, self.verifier, 5, None, dirty)
        self.assertEqual(status, 5)
        self.assertTrue((dirty_results / "verify-offline.json").is_file())
        cleanup = json.loads((dirty_results / "offline-cleanup.json").read_text(encoding="utf-8"))
        self.assertEqual(cleanup["status"], "dirty")
        self.assertIn("not proof the registration is gone", cleanup["profile_registration_removal"])

    def test_accept_while_contained_is_not_denial(self) -> None:
        outcomes = self._happy_outcomes()
        outcomes[3] = {
            "accept": True,
            "exit": 3,
            "stdout": WINDOWS_TIMEOUT_RECEIPT.encode(),
            "token": ZERO_TOKEN,
        }
        launcher = ScriptedLauncher(outcomes)
        status = self._run(launcher)
        self.assertNotEqual(status, 0)
        row = self._operation("isolated-probe")
        self.assertNotEqual(row["classification"], "network-denied")
        self.assertGreater(row["legs"][0]["listener_accepts"], 0)
        self.assertFalse((self.results / "verify-offline.json").exists())

    def test_prototype_table_pinned(self) -> None:
        module = load_windows()
        self.assertEqual(module.PROTOTYPES, PINNED_PROTOTYPES)
        applied = module.applied_prototype_names()
        self.assertEqual(applied, PINNED_PROTOTYPES)


PINNED_PROTOTYPES = {
    "advapi32.ConvertSidToStringSidW": ("BOOL", ("c_void_p", "LP_c_wchar_p")),
    "advapi32.ConvertStringSidToSidW": ("BOOL", ("c_wchar_p", "LP_c_void_p")),
    "advapi32.FreeSid": ("c_void_p", ("c_void_p",)),
    "advapi32.GetTokenInformation": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD", "LP_DWORD")),
    "advapi32.OpenProcessToken": ("BOOL", ("c_void_p", "DWORD", "LP_c_void_p")),
    "kernel32.AssignProcessToJobObject": ("BOOL", ("c_void_p", "c_void_p")),
    "kernel32.CloseHandle": ("BOOL", ("c_void_p",)),
    "kernel32.CreateFileW": (
        "c_void_p",
        ("c_wchar_p", "DWORD", "DWORD", "c_void_p", "DWORD", "DWORD", "c_void_p"),
    ),
    "kernel32.CreateIoCompletionPort": ("c_void_p", ("c_void_p", "c_void_p", "c_size_t", "DWORD")),
    "kernel32.CreateJobObjectW": ("c_void_p", ("c_void_p", "c_wchar_p")),
    "kernel32.CreatePipe": ("BOOL", ("LP_c_void_p", "LP_c_void_p", "c_void_p", "DWORD")),
    "kernel32.CreateProcessW": (
        "BOOL",
        (
            "c_wchar_p",
            "c_wchar_p",
            "c_void_p",
            "c_void_p",
            "BOOL",
            "DWORD",
            "c_void_p",
            "c_wchar_p",
            "c_void_p",
            "c_void_p",
        ),
    ),
    "kernel32.DeleteProcThreadAttributeList": ("None", ("c_void_p",)),
    "kernel32.GetCurrentProcess": ("c_void_p", ()),
    "kernel32.GetExitCodeProcess": ("BOOL", ("c_void_p", "LP_DWORD")),
    "kernel32.GetLastError": ("DWORD", ()),
    "kernel32.GetQueuedCompletionStatus": (
        "BOOL",
        ("c_void_p", "LP_DWORD", "LP_c_void_p", "LP_c_void_p", "DWORD"),
    ),
    "kernel32.InitializeProcThreadAttributeList": ("BOOL", ("c_void_p", "DWORD", "DWORD", "LP_c_size_t")),
    "kernel32.LocalFree": ("c_void_p", ("c_void_p",)),
    "kernel32.QueryInformationJobObject": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD", "LP_DWORD")),
    "kernel32.ReadFile": ("BOOL", ("c_void_p", "c_void_p", "DWORD", "LP_DWORD", "c_void_p")),
    "kernel32.ResumeThread": ("DWORD", ("c_void_p",)),
    "kernel32.SetHandleInformation": ("BOOL", ("c_void_p", "DWORD", "DWORD")),
    "kernel32.SetInformationJobObject": ("BOOL", ("c_void_p", "c_int", "c_void_p", "DWORD")),
    "kernel32.TerminateJobObject": ("BOOL", ("c_void_p", "UINT")),
    "kernel32.TerminateProcess": ("BOOL", ("c_void_p", "UINT")),
    "kernel32.UpdateProcThreadAttribute": (
        "BOOL",
        ("c_void_p", "DWORD", "c_size_t", "c_void_p", "c_size_t", "c_void_p", "LP_c_size_t"),
    ),
    "kernel32.WaitForSingleObject": ("DWORD", ("c_void_p", "DWORD")),
    "ole32.CoTaskMemFree": ("None", ("c_void_p",)),
    "userenv.CreateAppContainerProfile": (
        "c_long",
        ("c_wchar_p", "c_wchar_p", "c_wchar_p", "c_void_p", "DWORD", "LP_c_void_p"),
    ),
    "userenv.DeleteAppContainerProfile": ("c_long", ("c_wchar_p",)),
    "userenv.GetAppContainerFolderPath": ("c_long", ("c_wchar_p", "LP_c_wchar_p")),
}


def load_windows():
    path = ROOT / "scripts/ci/published_release_offline_windows.py"
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", path)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class ContractHookSelectorTests(unittest.TestCase):
    def test_helper_only_and_test_only_select_the_contract_hook(self) -> None:
        include, exclude = configured_contract_hook_selector(
            (ROOT / ".pre-commit-config.yaml").read_text(encoding="utf-8")
        )
        helper = "scripts/ci/published_release_offline_phase.py"
        windows = "scripts/ci/published_release_offline_windows.py"
        tests = "scripts/ci/test_published_release_offline_phase.py"
        self.assertTrue(contract_hook_selected(include, exclude, [helper]), helper)
        self.assertTrue(contract_hook_selected(include, exclude, [windows]), windows)
        self.assertTrue(contract_hook_selected(include, exclude, [tests]), tests)
        unrelated = (
            "README.md",
            "docs/LAUNCH.md",
            "crates/assay-cli/src/main.rs",
            "scripts/ci/published_release_offline_phase.py.bak",
            "scripts/ci/published_release_offline_windows.py.bak",
            "scripts/ci/test_published_release_offline_phase.py.bak",
        )
        for path in unrelated:
            self.assertFalse(contract_hook_selected(include, exclude, [path]), path)


if __name__ == "__main__":
    unittest.main()
