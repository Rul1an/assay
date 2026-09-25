#!/usr/bin/env python3
"""Dispatch measurement: can a zero-capability child leave a receipt in the production layout?

The harness records whether stdout is one offline-probe receipt, or a setup failure.
It does not classify the receipt. A written result is not Windows completion of #3148.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import threading

PROBE_SCHEMA = "assay.offline_probe.v1"
PROBE_RESULTS = {"connected", "denied", "timeout", "error"}
MAX_RECEIPT_BYTES = 65536
HEAD = 300
OUTCOME_RECEIPT = "receipt appeared"
OUTCOME_SETUP = "setup failure"


def receipt_appeared(stdout):
    """Same shape as published_release_offline_phase.parse_receipt. True or false only."""
    if not isinstance(stdout, (bytes, bytearray)) or len(stdout) > MAX_RECEIPT_BYTES:
        return False
    try:
        text = bytes(stdout).decode("utf-8")
    except UnicodeError:
        return False
    line, separator, extra = text.partition("\n")
    if separator != "\n" or extra.strip() or not line:
        return False
    try:
        receipt = json.loads(line)
    except json.JSONDecodeError:
        return False
    if not isinstance(receipt, dict) or receipt.get("schema") != PROBE_SCHEMA:
        return False
    if not isinstance(receipt.get("errno"), str):
        return False
    return receipt.get("result") in PROBE_RESULTS


def _self_test():
    failures = []
    line = b""
    for result, err in (
        ("timeout", "ETIMEDOUT"),
        ("connected", ""),
        ("denied", "EACCES"),
        ("error", ""),
    ):
        payload = {"errno": err, "result": result, "schema": PROBE_SCHEMA}
        line = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode() + b"\n"
        if not receipt_appeared(line):
            failures.append(result)
    if receipt_appeared(b"") or receipt_appeared(b"{}\n") or receipt_appeared(line + b"extra\n"):
        failures.append("reject")
    if receipt_appeared(line.rstrip(b"\n")):
        failures.append("newline")
    if failures:
        print("self-test exit 1 " + ",".join(failures))
        return 1
    print("self-test exit 0")
    return 0


def _head(data):
    if isinstance(data, bytes):
        data = data.decode("utf-8", "replace")
    if not isinstance(data, str):
        return ""
    return data[:HEAD]


def _flag(argv, name):
    if name not in argv or argv.index(name) + 1 >= len(argv):
        raise RuntimeError("missing " + name)
    return Path(argv[argv.index(name) + 1])


class _ReadyListener:
    """Harness listener. The child must read the ready byte to emit connected."""

    def __init__(self):
        self._sock = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        self._sock.bind(("127.0.0.1", 0))
        self._sock.listen(1)
        self._sock.settimeout(0.2)
        self.port = self._sock.getsockname()[1]
        self._closed = False
        self._thread = threading.Thread(target=self._serve, daemon=True)
        self._thread.start()

    def _serve(self):
        while not self._closed:
            try:
                connection, _addr = self._sock.accept()
            except OSError:
                continue
            try:
                connection.sendall(b"ready")
            finally:
                connection.close()

    def close(self):
        self._closed = True
        try:
            self._sock.close()
        except OSError:
            pass
        self._thread.join(timeout=1)


def _sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _child_exit(launched):
    wait = launched.get("wait_result")
    if wait in ("timeout", "still-running"):
        return wait
    return launched.get("exit")


def main(argv):
    if "--self-test" in argv:
        return _self_test()
    import windows_appcontainer_probe as probe

    out = Path("results/windows-offline-layout.json").resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    document = {
        "outcome": OUTCOME_SETUP,
        "reason": "not recorded",
        "child_exit": None,
        "stderr_head": None,
        "stdout_head": None,
        "cleanup": None,
    }
    state = {
        "audit_prior": None,
        "profile": None,
        "grants": None,
        "acquired": [],
        "closer": probe._close_launch_item,
    }
    listener = None
    try:
        document["home"] = os.environ.get("HOME")
        script = _flag(argv, "--script").resolve()
        bundle = _flag(argv, "--bundle").resolve()
        verifier = _flag(argv, "--verifier").resolve()
        for path in (script, bundle, verifier):
            if not path.is_file():
                raise RuntimeError("missing file " + str(path))
        # NULL cwd in launch_in_profile inherits this process. Production cwd is
        # the results directory, which is the bundle's parent and one of the grants.
        os.chdir(bundle.parent)
        profile = probe.create_profile_once()
        state["profile"] = profile
        document["profile_sid"] = profile["sid"]
        grants = []
        state["grants"] = grants
        document["grants"] = grants
        probe.grant_paths(
            profile["sid"],
            [
                (Path(sys.base_prefix), True),
                (script, False),
                (script.parent, True),
                (bundle, False),
                (bundle.parent, True),
                (verifier, False),
                (verifier.parent, True),
            ],
            grants,
        )
        listener = _ReadyListener()
        argv_child = [
            sys.executable,
            "-I",
            str(script),
            "--probe",
            "127.0.0.1",
            str(listener.port),
        ]
        document["argv"] = argv_child
        document["cwd"] = str(Path.cwd())
        document["base_prefix"] = sys.base_prefix
        document["executable"] = sys.executable
        document["script_sha256"] = _sha256(script)
        document["script_blob"] = os.environ.get("LAYOUT_SCRIPT_BLOB")
        launched = probe.launch_in_profile(
            profile["sid"],
            None,
            argv_child,
            probe.child_environment(os.environ),
            30,
            state["acquired"],
            label="layout",
        )
        document["stdout_head"] = _head(launched.get("stdout"))
        if receipt_appeared(launched.get("stdout")):
            document["outcome"] = OUTCOME_RECEIPT
            document["reason"] = None
        else:
            document["outcome"] = OUTCOME_SETUP
            document["reason"] = "receipt missing"
            document["child_exit"] = _child_exit(launched)
            document["stderr_head"] = _head(launched.get("stderr"))
    except Exception as exc:
        document["outcome"] = OUTCOME_SETUP
        document["reason"] = str(exc)[:HEAD]
        launch = getattr(exc, "launch", None)
        if isinstance(launch, dict):
            document["child_exit"] = launch.get("exit")
            document["stderr_head"] = _head(launch.get("stderr"))
    finally:
        if listener is not None:
            listener.close()
        try:
            document["cleanup"] = probe.cleanup(state)
        except Exception as exc:
            document["cleanup"] = {"status": "unknown", "error": str(exc)[:HEAD]}
        out.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv[1:]))
