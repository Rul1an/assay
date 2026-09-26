#!/usr/bin/env python3
"""Run the published-release offline verifier under one Linux unshare constructor.

The connected probe and the isolated probe are the same executable. A nonzero
child status is network denial only when that probe leaves its own receipt.
Timeout, a missing executable, and a namespace launch failure stop before the
verifier. This is not a generic isolation framework.
"""

from __future__ import annotations

import errno
import importlib.util
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
# Measured WSAEACCES is 10013. CPython maps it to EACCES. The name stays off
# DENIAL_ERRNOS; only the AppContainer constructor accepts the pair.
WINDOWS_DENIAL = frozenset({("EACCES", 10013)})
WINDOWS_ZERO_CAPABILITIES: list[str] = []
WINDOWS_INTERNET_CLIENT_CAPABILITIES = ["S-1-15-3-1"]
CLEANUP_DIRTY_EXIT = 5
WINDOWS_OFFLINE_CLAIM = (
    "On Windows x86_64 the published verifier reproduced the connected `verify.json` "
    "byte-for-byte while running as a zero-capability AppContainer process (moniker "
    "profile, no package identity). In that same container the harness's probe was "
    "refused an outbound TCP connection to the harness-recorded external address "
    "(WSAEACCES) and did not complete a TCP connection to the harness loopback "
    "listener (the listener accepted none), while the host and an "
    "internetClient-capability launch through the same launcher connected. Name "
    "resolution inside the container failed. UDP was not measured as denied. This "
    "shows the verifier needs no TCP network to verify; it does not identify the "
    "filters and does not show that datagrams could not leave the container."
)
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
    "permissive-control-failed": "internetClient control did not connect; offline verifier was not run",
    "cleanup-dirty": "offline cleanup was not clean",
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
    # EPERM and EACCES are emitted so a platform allow-list can see them.
    # Neither name joins DENIAL_ERRNOS.
    if error.errno in DENIAL_ERRNOS or error.errno in {errno.EPERM, errno.EACCES}:
        return "denied", name, DENIAL_EXIT
    return "error", name, 5


def _isolation_kind(isolation: list[str] | dict | None) -> str:
    if isinstance(isolation, dict):
        kind = isolation.get("kind")
        if kind in {"appcontainer", "host", "sandbox-exec", "unshare"}:
            return str(kind)
        return ""
    if isolation and len(isolation) >= 2 and isolation[0] == SANDBOX_EXEC and isolation[1] == "-p":
        return "sandbox-exec"
    return "unshare"


def denial_names_for(isolation: list[str] | dict | None) -> frozenset[str]:
    kind = _isolation_kind(isolation)
    if kind == "sandbox-exec":
        return DARWIN_DENIAL_ERRNO_NAMES
    if kind == "appcontainer":
        return frozenset(name for name, _winerror in WINDOWS_DENIAL)
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
        self._accepts = 0
        self._accept_lock = threading.Lock()
        self._thread = threading.Thread(target=self._serve, name="offline-probe-listener", daemon=True)
        self._thread.start()

    def accept_count(self) -> int:
        """Connections this process accepted. The child does not report this."""
        with self._accept_lock:
            return self._accepts

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
            with self._accept_lock:
                self._accepts += 1
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


def emit_receipt(result: str, err: str, winerror: int | None = None) -> None:
    payload = {"errno": err, "result": result, "schema": PROBE_SCHEMA}
    if isinstance(winerror, int) and not isinstance(winerror, bool):
        payload["winerror"] = winerror
    sys.stdout.write(json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n")


def run_probe(host: str, port: int, timeout: float, connect_only: bool = False) -> int:
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
        winerror = getattr(error, "winerror", None)
        if is_socket_timeout(error):
            emit_receipt("timeout", "ETIMEDOUT", winerror)
            return PROBE_TIMEOUT_EXIT
        result, name, status = classify_probe_oserror(error)
        emit_receipt(result, name, winerror)
        return status
    if connect_only:
        emit_receipt("connected", "")
        return 0
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
    if set(receipt) - {"errno", "result", "schema", "winerror"}:
        return None
    winerror = receipt.get("winerror")
    if "winerror" in receipt and (isinstance(winerror, bool) or not isinstance(winerror, int)):
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
    isolation: list[str] | dict | None = None,
    listener_accepts: int | None = None,
) -> str:
    if exit_code == HARNESS_TIMEOUT_EXIT:
        return "timeout"
    if len(stdout) > MAX_CAPTURE_BYTES or len(stderr) > MAX_CAPTURE_BYTES:
        return "unexpected-exit"
    receipt = parse_receipt(stdout)
    if receipt is None:
        return "isolate-setup" if not stdout else "unexpected-exit"
    if receipt["result"] == "timeout" or exit_code == PROBE_TIMEOUT_EXIT:
        # A loopback timeout is denial only when this process accepted nothing.
        # The same receipt under unshare stays a timeout.
        if _isolation_kind(isolation) == "appcontainer" and listener_accepts == 0:
            return "network-denied"
        if (
            _isolation_kind(isolation) == "appcontainer"
            and isinstance(listener_accepts, int)
            and not isinstance(listener_accepts, bool)
            and listener_accepts > 0
        ):
            return "unexpected-exit"
        return "timeout"
    if exit_code == 0 and receipt["result"] == "connected":
        return "isolated-connected"
    if (
        exit_code == DENIAL_EXIT
        and receipt["result"] == "denied"
        and receipt["errno"] in denial_names_for(isolation)
    ):
        if _isolation_kind(isolation) == "appcontainer":
            if (receipt["errno"], receipt.get("winerror")) not in WINDOWS_DENIAL:
                return "unexpected-exit"
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


def _bytes(value: object) -> bytes:
    if isinstance(value, bytes):
        return value
    if isinstance(value, str):
        return value.encode()
    return b""


def execute(
    argv: list[str],
    timeout: int,
    *,
    launcher: object | None = None,
    capabilities: list[str] | None = None,
) -> tuple[int, bytes, bytes]:
    if launcher is not None:
        result = launcher.launch(argv, os.environ, timeout, capabilities)
        recorded = result if isinstance(result, dict) else {}
        launcher.last_result = recorded
        stdout = _bytes(recorded.get("stdout"))
        stderr = _bytes(recorded.get("stderr"))
        if recorded.get("create_process") is False:
            return 1, stdout, stderr
        if recorded.get("wait_result") in {"still-running", "timeout"}:
            launcher.terminate_job(recorded)
            return HARNESS_TIMEOUT_EXIT, stdout, stderr
        status = recorded.get("exit")
        if isinstance(status, bool) or not isinstance(status, int):
            status = 1
        return status, stdout, stderr
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
    extra: dict | None = None,
) -> None:
    operation = {
        "argv": argv,
        "classification": classification,
        "exit_code": exit_code,
        "name": name,
        "stderr": stderr[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
        "stdout": stdout[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
    }
    if extra:
        operation.update(extra)
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


def _production_launcher():
    path = Path(__file__).with_name("published_release_offline_windows.py")
    spec = importlib.util.spec_from_file_location("published_release_offline_windows", path)
    if spec is None or spec.loader is None:
        raise FileNotFoundError(path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module.ProductionLauncher()


def _windows_grant_paths(verifier: list[str], results: Path) -> list[str] | None:
    binary = Path(verifier[0])
    if not binary.is_absolute():
        return None
    paths = [
        Path(sys.base_prefix).resolve(),
        Path(__file__).resolve().parent,
        binary.resolve().parent,
        results.resolve(),
    ]
    if len(set(paths)) != len(paths):
        return None
    for path in paths:
        for other in paths:
            if path != other and path in other.parents:
                return None
    return [str(path) for path in paths]


def _resolve_external_ipv4(host: str = "github.com", port: int = 443) -> str:
    infos = socket.getaddrinfo(host, port, socket.AF_INET, socket.SOCK_STREAM)
    if not infos:
        raise OSError("external address unresolved")
    address = infos[0][4][0]
    socket.inet_aton(address)
    return address


def _windows_probe_argv(
    port: int,
    probe_executable: str | None,
    *,
    connect_only: bool = False,
    external: str | None = None,
) -> list[str]:
    argv = probe_argv(port, probe_executable)
    if connect_only:
        argv.append("--connect-only")
    if external is not None:
        argv.extend(["--external", external])
    return argv


def _token_matches(token: object, capabilities: list[str] | None, profile_sid: str) -> bool:
    if not isinstance(token, dict):
        return False
    if capabilities is None:
        return token.get("is_app_container") is False and token.get("capabilities") == []
    if token.get("is_app_container") is not True or token.get("sid") != profile_sid:
        return False
    return token.get("capabilities") == list(capabilities)


def _windows_descriptor(profile_sid: str, capabilities: list[str] | None) -> dict:
    if capabilities is None:
        return {"kind": "host"}
    return {"capabilities": list(capabilities), "kind": "appcontainer", "profile_sid": profile_sid}


def _record_windows(
    results: Path,
    name: str,
    argv: list[str],
    exit_code: int,
    stdout: bytes,
    stderr: bytes,
    classification: str,
    isolation: dict,
    external: str,
    legs: list[dict],
    last_error: int | None,
) -> None:
    extra = {"external_address": external, "isolation": isolation, "legs": legs}
    if last_error is not None:
        extra["last_error"] = last_error
    append_record(results, name, argv, exit_code, stdout, stderr, classification, extra)


def _windows_leg(
    argv: list[str],
    timeout: int,
    launcher: object,
    capabilities: list[str] | None,
    profile_sid: str,
    listener: LoopbackListener,
    count_accepts: bool,
    classify,
) -> tuple[str, int, bytes, bytes, dict, int | None]:
    before = listener.accept_count()
    exit_code, stdout, stderr = execute(argv, timeout, launcher=launcher, capabilities=capabilities)
    result = getattr(launcher, "last_result", {}) or {}
    accepts = listener.accept_count() - before if count_accepts else None
    last_error = result.get("last_error")
    if isinstance(last_error, bool) or not isinstance(last_error, int):
        last_error = None
    if result.get("create_process") is False:
        classification = "isolate-setup"
    elif not _token_matches(result.get("token"), capabilities, profile_sid):
        classification = "isolate-setup"
    elif exit_code == HARNESS_TIMEOUT_EXIT:
        classification = "timeout"
    else:
        classification = classify(exit_code, stdout, stderr, accepts)
    leg = {
        "argv": argv,
        "classification": classification,
        "exit_code": exit_code,
        "job_processes": result.get("job_processes") or [],
        "job_total_processes": result.get("job_total_processes"),
        "listener_accepts": accepts,
        "stdout": stdout[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
        "token": result.get("token"),
    }
    return classification, exit_code, stdout, stderr, leg, last_error


def _windows_arms(
    results: Path,
    verifier: list[str],
    timeout: int,
    probe_executable: str | None,
    launcher: object,
    listener: LoopbackListener,
    state: dict,
    external: str,
) -> int:
    profile = state.get("profile") if isinstance(state, dict) else None
    profile_sid = profile.get("sid") if isinstance(profile, dict) else ""
    if not isinstance(profile_sid, str):
        profile_sid = ""
    external_arg = f"{external}:443"
    host = None
    client = WINDOWS_INTERNET_CLIENT_CAPABILITIES
    zero = WINDOWS_ZERO_CAPABILITIES
    host_descriptor = _windows_descriptor(profile_sid, host)
    client_descriptor = _windows_descriptor(profile_sid, client)
    zero_descriptor = _windows_descriptor(profile_sid, zero)
    loop = _windows_probe_argv(listener.port, probe_executable)
    external_argv = _windows_probe_argv(
        listener.port, probe_executable, connect_only=True, external=external_arg
    )

    def host_loopback(exit_code, stdout, stderr, accepts):
        classification = classify_connected(exit_code, stdout, stderr)
        if classification == "connected" and accepts != 1:
            return "connected-failure"
        return classification

    def host_external(exit_code, stdout, stderr, _accepts):
        return classify_connected(exit_code, stdout, stderr)

    def permissive(exit_code, stdout, stderr, _accepts):
        classification = classify_connected(exit_code, stdout, stderr)
        if classification == "connected" or classification in {"missing-probe", "timeout"}:
            return classification
        return "permissive-control-failed"

    def isolated_loopback(exit_code, stdout, stderr, accepts):
        return classify_isolated(exit_code, stdout, stderr, zero_descriptor, listener_accepts=accepts)

    def isolated_external(exit_code, stdout, stderr, _accepts):
        return classify_isolated(exit_code, stdout, stderr, zero_descriptor)

    loop_class, loop_exit, loop_out, loop_err, loop_leg, loop_error = _windows_leg(
        loop, timeout, launcher, host, profile_sid, listener, True, host_loopback
    )
    if loop_class != "connected":
        _record_windows(
            results, "connected-probe", loop, loop_exit, loop_out, loop_err, loop_class,
            host_descriptor, external, [loop_leg], loop_error,
        )
        return finish(loop_class)
    ext_class, ext_exit, ext_out, ext_err, ext_leg, ext_error = _windows_leg(
        external_argv, timeout, launcher, host, profile_sid, listener, False, host_external
    )
    connected_class = ext_class if ext_class != "connected" else "connected"
    _record_windows(
        results, "connected-probe", loop, loop_exit, loop_out, loop_err, connected_class,
        host_descriptor, external, [loop_leg, ext_leg], ext_error or loop_error,
    )
    if connected_class != "connected":
        return finish(connected_class)
    permit_class, permit_exit, permit_out, permit_err, permit_leg, permit_error = _windows_leg(
        external_argv, timeout, launcher, client, profile_sid, listener, False, permissive
    )
    _record_windows(
        results, "isolated-permissive-probe", external_argv, permit_exit, permit_out, permit_err,
        permit_class, client_descriptor, external, [permit_leg], permit_error,
    )
    if permit_class != "connected":
        return finish(permit_class)
    deny_class, deny_exit, deny_out, deny_err, deny_leg, deny_error = _windows_leg(
        loop, timeout, launcher, zero, profile_sid, listener, True, isolated_loopback
    )
    if deny_class != "network-denied":
        _record_windows(
            results, "isolated-probe", loop, deny_exit, deny_out, deny_err, deny_class,
            zero_descriptor, external, [deny_leg], deny_error,
        )
        return finish(deny_class)
    out_class, out_exit, out_out, out_err, out_leg, out_error = _windows_leg(
        external_argv, timeout, launcher, zero, profile_sid, listener, False, isolated_external
    )
    isolated_class = "network-denied" if out_class == "network-denied" else out_class
    _record_windows(
        results, "isolated-probe", loop, deny_exit, deny_out, deny_err, isolated_class,
        zero_descriptor, external, [deny_leg, out_leg], out_error or deny_error,
    )
    if isolated_class != "network-denied":
        return finish(isolated_class)
    verified_exit, verified_out, verified_err = execute(
        verifier, timeout, launcher=launcher, capabilities=zero
    )
    verified_result = getattr(launcher, "last_result", {}) or {}
    verified_error = verified_result.get("last_error")
    if isinstance(verified_error, bool) or not isinstance(verified_error, int):
        verified_error = None
    if verified_result.get("create_process") is False or not _token_matches(
        verified_result.get("token"), zero, profile_sid
    ):
        verified_class = "isolate-setup"
    elif verified_exit == HARNESS_TIMEOUT_EXIT:
        verified_class = "timeout"
    elif verified_exit == 0 and not verified_result.get("truncated"):
        verified_class = "verified"
    else:
        verified_class = "unexpected-exit"
    verified_leg = {
        "argv": verifier,
        "classification": verified_class,
        "exit_code": verified_exit,
        "job_processes": verified_result.get("job_processes") or [],
        "job_total_processes": verified_result.get("job_total_processes"),
        "listener_accepts": None,
        "stdout": verified_out[:MAX_CAPTURE_BYTES].decode("utf-8", "replace"),
        "token": verified_result.get("token"),
    }
    _record_windows(
        results, "verify-produced-bundle-offline", verifier, verified_exit, verified_out, verified_err,
        verified_class, zero_descriptor, external, [verified_leg], verified_error,
    )
    (results / "verify-offline.json").write_bytes(verified_out)
    (results / "verify-offline.stderr").write_bytes(verified_err)
    if verified_class != "verified":
        if verified_class == "unexpected-exit":
            print(FAILURE_TEXT["unexpected-exit"], file=sys.stderr)
            return verified_exit if 0 < verified_exit <= 255 else 1
        return finish(verified_class)
    return 0


def _run_windows(
    results: Path,
    verifier: list[str],
    timeout: int,
    probe_executable: str | None,
    launcher: object | None,
) -> int:
    if launcher is None:
        launcher = _production_launcher()
    listener = LoopbackListener()
    (results / "offline-listener.port").write_text(f"{listener.port}\n", encoding="utf-8")
    state: dict = {"grants": [], "profile": None}
    outcome = {"status": 1}
    try:
        paths = _windows_grant_paths(verifier, results)
        if paths is None:
            append_record(
                results, "connected-probe", verifier, 1, b"", b"grant paths overlap\n", "isolate-setup"
            )
            outcome["status"] = finish("isolate-setup")
        else:
            try:
                external = _resolve_external_ipv4()
            except OSError as exc:
                append_record(
                    results, "connected-probe", verifier, 1, b"", str(exc).encode(), "isolate-setup"
                )
                outcome["status"] = finish("isolate-setup")
            else:
                try:
                    prepared = launcher.prepare(paths)
                except Exception as exc:
                    held = getattr(launcher, "state", None)
                    if isinstance(held, dict):
                        state = held
                    append_record(
                        results, "connected-probe", verifier, 1, b"", str(exc).encode(), "isolate-setup"
                    )
                    outcome["status"] = finish("isolate-setup")
                else:
                    if isinstance(prepared, dict):
                        state = prepared
                    outcome["status"] = _windows_arms(
                        results, verifier, timeout, probe_executable, launcher, listener, state, external
                    )
    finally:
        listener.close()
        try:
            report = launcher.cleanup(state)
            if not isinstance(report, dict):
                report = {"status": "unknown"}
        except Exception as exc:
            report = {"error": str(exc), "status": "unknown"}
        report = dict(report)
        report["profile_registration_removal"] = (
            "DeleteAppContainerProfile success is not proof the registration is gone"
        )
        (results / "offline-cleanup.json").write_text(
            json.dumps(report, sort_keys=True) + "\n",
            encoding="utf-8",
        )
        if report.get("status") != "clean":
            print(FAILURE_TEXT["cleanup-dirty"], file=sys.stderr)
            outcome["status"] = CLEANUP_DIRTY_EXIT
    return outcome["status"]


def run_offline_phase(
    results: Path,
    verifier: list[str],
    timeout: int,
    probe_executable: str | None,
    launcher: object | None = None,
) -> int:
    if sys.platform == "win32":
        return _run_windows(results, verifier, timeout, probe_executable, launcher)
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
        connect_only = False
        external = None
        if "--connect-only" in args:
            connect_only = True
            args.remove("--connect-only")
        if "--external" in args:
            marker = args.index("--external")
            if marker + 1 >= len(args):
                raise SystemExit("missing value for --external")
            external = args[marker + 1]
            del args[marker : marker + 2]
            connect_only = True
        if "--probe-timeout" in args:
            marker = args.index("--probe-timeout")
            timeout = float(args[marker + 1])
            del args[marker : marker + 2]
        if len(args) != 3:
            raise SystemExit("usage: published_release_offline_phase.py --probe HOST PORT")
        host, port = args[1], int(args[2])
        if external is not None:
            host, separator, port_text = external.rpartition(":")
            if separator != ":" or not port_text.isdigit() or not host:
                raise SystemExit("external must be HOST:PORT")
            port = int(port_text)
        return run_probe(host, port, timeout, connect_only=connect_only)
    results, timeout, verifier = parse_phase(args)
    return run_offline_phase(results, verifier, timeout, None)


if __name__ == "__main__":
    raise SystemExit(main())
