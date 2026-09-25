#!/usr/bin/env python3
"""Run the published-release offline verifier under one Linux unshare constructor.

The connected probe and the isolated probe are the same executable. A nonzero
child status is network denial only when that probe leaves its own receipt.
Timeout, a missing executable, and a namespace launch failure stop before the
verifier. This is not a generic isolation framework.
"""

from __future__ import annotations

import errno
import json
import os
from pathlib import Path
import signal
import socket
import subprocess
import sys
import threading


MAX_CAPTURE_BYTES = 65536
DENIAL_EXIT = 4
PROBE_TIMEOUT_EXIT = 3
HARNESS_TIMEOUT_EXIT = 124
MISSING_EXIT = 127
PROBE_SCHEMA = "assay.offline_probe.v1"
DEFAULT_PROBE_TIMEOUT = 2.0
DENIAL_ERRNOS = {
    errno.ECONNREFUSED,
    errno.ENETUNREACH,
    errno.EHOSTUNREACH,
    errno.ENETDOWN,
}
DENIAL_ERRNO_NAMES = frozenset(
    name for name in (errno.errorcode.get(number) for number in DENIAL_ERRNOS) if name
)
# Measured on hosted macOS by the sandbox-exec feasibility probe: connect
# under the restrictive profile returns EPERM. That name stays off DENIAL_ERRNOS.
DARWIN_DENIAL_ERRNO_NAMES = frozenset({"EPERM"})
SANDBOX_EXEC = "/usr/bin/sandbox-exec"
DARWIN_RESTRICTIVE_PROFILE = "(version 1)(allow default)(deny network*)"
DARWIN_PERMISSIVE_PROFILE = "(version 1)(allow default)(allow network*)"
FAILURE_STATUS = {
    "missing-probe": MISSING_EXIT,
    "timeout": HARNESS_TIMEOUT_EXIT,
}
FAILURE_TEXT = {
    "connected-failure": "connected probe failed; offline verifier was not run",
    "missing-probe": "probe executable is missing; offline verifier was not run",
    "isolate-setup": "isolation setup failed; offline verifier was not run",
    "timeout": "isolation probe timed out; timeout is not network denial",
    "unexpected-exit": "isolation probe returned an unexpected result; offline verifier was not run",
    "isolated-connected": "network isolation control failed: probe connected inside the namespace",
}


def isolation_argv(command: list[str]) -> list[str]:
    if sys.platform == "darwin":
        return [SANDBOX_EXEC, "-p", DARWIN_RESTRICTIVE_PROFILE, *command]
    return ["unshare", "-rn", *command]


def classify_probe_oserror(error: OSError) -> tuple[str, str, int]:
    """Turn one connect failure into a probe receipt. Acceptance is separate.

    EPERM is emitted as ``denied`` so the Darwin allow-list can see the
    measured receipt. It is not a member of the Linux denial set.
    """
    name = errno.errorcode.get(error.errno or 0, "")
    if error.errno in DENIAL_ERRNOS or error.errno == errno.EPERM:
        return "denied", name, DENIAL_EXIT
    return "error", name, 5


def denial_names_for(isolation: list[str] | None) -> frozenset[str]:
    if isolation and len(isolation) >= 2 and isolation[0] == SANDBOX_EXEC and isolation[1] == "-p":
        return DARWIN_DENIAL_ERRNO_NAMES
    return DENIAL_ERRNO_NAMES


def is_socket_timeout(error: BaseException) -> bool:
    """True when an accept or recv hit its deadline.

    Python 3.10+ aliases ``socket.timeout`` to ``TimeoutError``. Python 3.9,
    which Xcode's ``python3`` still is, raises ``socket.timeout``: an
    ``OSError`` that is not ``TimeoutError`` and carries no errno.
    """
    return isinstance(error, (TimeoutError, socket.timeout))


class LoopbackListener:
    """Local listener. A successful probe must read the ready byte."""

    def __init__(self) -> None:
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._sock.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
        self._sock.bind(("127.0.0.1", 0))
        self._sock.listen(1)
        self._sock.settimeout(0.2)
        self.port = self._sock.getsockname()[1]
        self._closed = False
        self._thread = threading.Thread(target=self._serve, name="offline-probe-listener", daemon=True)
        self._thread.start()

    def _serve(self) -> None:
        while not self._closed:
            try:
                connection, _ = self._sock.accept()
            except OSError as error:
                # A deadline, including Python 3.9's socket.timeout, is another
                # wait. Anything else means the listening socket is done.
                if not self._closed and is_socket_timeout(error):
                    continue
                return
            try:
                connection.sendall(b"ready")
            finally:
                connection.close()

    def close(self) -> None:
        self._closed = True
        try:
            self._sock.close()
        except OSError:
            pass
        self._thread.join(timeout=1)


def emit_receipt(result: str, err: str) -> None:
    payload = {"errno": err, "result": result, "schema": PROBE_SCHEMA}
    sys.stdout.write(json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n")


def run_probe(host: str, port: int, timeout: float) -> int:
    try:
        with socket.create_connection((host, port), timeout=timeout) as probe:
            probe.settimeout(timeout)
            data = b""
            while len(data) < len(b"ready"):
                chunk = probe.recv(len(b"ready") - len(data))
                if not chunk:
                    break
                data += chunk
    except OSError as error:
        if is_socket_timeout(error):
            emit_receipt("timeout", "ETIMEDOUT")
            return PROBE_TIMEOUT_EXIT
        result, name, status = classify_probe_oserror(error)
        emit_receipt(result, name)
        return status
    if data != b"ready":
        emit_receipt("error", "")
        return 5
    emit_receipt("connected", "")
    return 0


def parse_receipt(stdout: bytes) -> dict | None:
    if len(stdout) > MAX_CAPTURE_BYTES:
        return None
    try:
        text = stdout.decode("utf-8")
    except UnicodeError:
        return None
    line, separator, extra = text.partition("\n")
    if separator != "\n" or extra.strip() or not line:
        return None
    try:
        receipt = json.loads(line)
    except json.JSONDecodeError:
        return None
    if not isinstance(receipt, dict):
        return None
    if receipt.get("schema") != PROBE_SCHEMA or not isinstance(receipt.get("errno"), str):
        return None
    if receipt.get("result") not in {"connected", "denied", "timeout", "error"}:
        return None
    return receipt


def classify_connected(exit_code: int, stdout: bytes, stderr: bytes) -> str:
    if len(stdout) > MAX_CAPTURE_BYTES or len(stderr) > MAX_CAPTURE_BYTES:
        return "connected-failure"
    if exit_code == MISSING_EXIT:
        return "missing-probe"
    if exit_code == HARNESS_TIMEOUT_EXIT:
        return "timeout"
    receipt = parse_receipt(stdout)
    if exit_code == 0 and receipt is not None and receipt["result"] == "connected":
        return "connected"
    return "connected-failure"


def classify_isolated(
    exit_code: int,
    stdout: bytes,
    stderr: bytes,
    isolation: list[str] | None = None,
) -> str:
    if exit_code == HARNESS_TIMEOUT_EXIT:
        return "timeout"
    if len(stdout) > MAX_CAPTURE_BYTES or len(stderr) > MAX_CAPTURE_BYTES:
        return "unexpected-exit"
    receipt = parse_receipt(stdout)
    if receipt is None:
        return "isolate-setup" if not stdout else "unexpected-exit"
    if receipt["result"] == "timeout" or exit_code == PROBE_TIMEOUT_EXIT:
        return "timeout"
    if exit_code == 0 and receipt["result"] == "connected":
        return "isolated-connected"
    if (
        exit_code == DENIAL_EXIT
        and receipt["result"] == "denied"
        and receipt["errno"] in denial_names_for(isolation)
    ):
        return "network-denied"
    return "unexpected-exit"


def kill_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait()


def execute(argv: list[str], timeout: int) -> tuple[int, bytes, bytes]:
    try:
        process = subprocess.Popen(
            argv,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            start_new_session=True,
        )
    except FileNotFoundError:
        return MISSING_EXIT, b"", f"{argv[0]}: not found\n".encode()
    try:
        stdout, stderr = process.communicate(timeout=timeout)
    except subprocess.TimeoutExpired:
        kill_group(process)
        stdout, stderr = process.communicate()
        return HARNESS_TIMEOUT_EXIT, stdout or b"", stderr or b""
    status = process.returncode if process.returncode is not None else 1
    return status, stdout, stderr


def append_record(
    results: Path,
    name: str,
    argv: list[str],
    exit_code: int,
    stdout: bytes,
    stderr: bytes,
    classification: str,
) -> None:
    operation = {
        "argv": argv,
        "classification": classification,
        "exit_code": exit_code,
        "name": name,
        "stderr": stderr[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
        "stdout": stdout[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
    }
    command = {"argv": argv, "exit_code": exit_code, "name": name}
    for path, row in (
        (results / "offline-operations.ndjson", operation),
        (results / "commands.ndjson", command),
    ):
        with path.open("a", encoding="utf-8") as handle:
            handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())


def finish(classification: str) -> int:
    print(FAILURE_TEXT[classification], file=sys.stderr)
    return FAILURE_STATUS.get(classification, 1)


def probe_argv(port: int, probe_executable: str | None) -> list[str]:
    executable = probe_executable or sys.executable
    argv = [executable]
    if probe_executable is None:
        argv.append("-I")
    argv.extend([str(Path(__file__).resolve()), "--probe", "127.0.0.1", str(port)])
    return argv


def run_offline_phase(
    results: Path,
    verifier: list[str],
    timeout: int,
    probe_executable: str | None,
) -> int:
    listener = LoopbackListener()
    (results / "offline-listener.port").write_text(f"{listener.port}\n", encoding="utf-8")
    try:
        command = probe_argv(listener.port, probe_executable)
        exit_code, stdout, stderr = execute(command, timeout)
        classification = classify_connected(exit_code, stdout, stderr)
        append_record(results, "connected-probe", command, exit_code, stdout, stderr, classification)
        if classification != "connected":
            return finish(classification)
        isolated = isolation_argv(command)
        exit_code, stdout, stderr = execute(isolated, timeout)
        classification = classify_isolated(exit_code, stdout, stderr, isolated)
        append_record(results, "isolated-probe", isolated, exit_code, stdout, stderr, classification)
        if classification != "network-denied":
            return finish(classification)
        verified = isolation_argv(verifier)
        exit_code, stdout, stderr = execute(verified, timeout)
        classification = "verified" if exit_code == 0 else "unexpected-exit"
        append_record(
            results,
            "verify-produced-bundle-offline",
            verified,
            exit_code,
            stdout,
            stderr,
            classification,
        )
        (results / "verify-offline.json").write_bytes(stdout)
        (results / "verify-offline.stderr").write_bytes(stderr)
        if classification != "verified":
            print(FAILURE_TEXT["unexpected-exit"], file=sys.stderr)
            return exit_code if 0 < exit_code <= 255 else 1
        return 0
    finally:
        listener.close()


def parse_phase(argv: list[str]) -> tuple[Path, int, list[str]]:
    if "--" not in argv:
        raise SystemExit("offline phase requires -- before the verifier argv")
    split = argv.index("--")
    options, verifier = argv[:split], argv[split + 1 :]
    if not verifier:
        raise SystemExit("offline phase requires the verifier argv")
    timeout = 30
    index = 0
    while index < len(options):
        token = options[index]
        if token == "--timeout-seconds":
            if index + 1 >= len(options):
                raise SystemExit(f"missing value for {token}")
            value = options[index + 1]
            index += 2
            timeout = int(value)
            if not 1 <= timeout <= 300:
                raise SystemExit("timeout must be between 1 and 300 seconds")
            continue
        raise SystemExit(f"unknown offline phase argument: {token}")
    return Path.cwd(), timeout, verifier


def main(argv: list[str] | None = None) -> int:
    args = list(sys.argv[1:] if argv is None else argv)
    if args[:1] == ["--probe"]:
        timeout = DEFAULT_PROBE_TIMEOUT
        if "--probe-timeout" in args:
            marker = args.index("--probe-timeout")
            timeout = float(args[marker + 1])
            del args[marker : marker + 2]
        if len(args) != 3:
            raise SystemExit("usage: published_release_offline_phase.py --probe HOST PORT")
        return run_probe(args[1], int(args[2]), timeout)
    results, timeout, verifier = parse_phase(args)
    return run_offline_phase(results, verifier, timeout, None)


if __name__ == "__main__":
    raise SystemExit(main())
