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
import socket
import stat
import subprocess
import sys
import tempfile
import textwrap
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
        argv = ["--results", str(self.results), "--timeout-seconds", str(timeout), "--"]
        if probe_executable is not None:
            argv[2:2] = ["--probe-executable", probe_executable]
        return self.helper.main(argv + VERIFIER)

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


if __name__ == "__main__":
    unittest.main()
