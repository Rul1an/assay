#!/usr/bin/env python3
"""Dispatch measurement: zero-capability start, read, and verifier execution.

The probe launch records whether stdout is one receipt whose result is not
connected. The verifier launch records whether the in-container stdout bytes
match a harness run of the same argv, both exits are 0, and the harness read
a zero-capability AppContainer token from the suspended process. A no-grant
control uses that same launcher before any grant. A written result is not
Windows completion of #3148.
"""

from __future__ import annotations

import hashlib
import json
import os
from pathlib import Path
import socket
import sys
import threading

import windows_offline_layout_compare as layout

HEAD = 300


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


def _sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def _child_exit(launched):
    if not isinstance(launched, dict):
        return None
    wait = launched.get("wait_result")
    if wait in ("timeout", "still-running"):
        return wait
    return launched.get("exit")


def _blank_launch():
    return {
        "outcome": layout.OUTCOME_SETUP,
        "reason": "not recorded",
        "child_exit": None,
        "stderr_head": None,
        "capability_sid": None,
        "token": None,
    }


def _fail_unrecorded(document, exc):
    reason = str(exc)[:HEAD]
    launch = getattr(exc, "launch", None)
    for record in document["launches"].values():
        if record.get("reason") != "not recorded":
            continue
        record["outcome"] = layout.OUTCOME_SETUP
        record["reason"] = reason
        if isinstance(launch, dict):
            record["child_exit"] = launch.get("exit")
            record["stderr_head"] = _head(launch.get("stderr"))


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


def _outside_capture(probe, argv):
    captured = probe.supervise_owned(
        probe._spawn_owned(argv, probe.child_environment(os.environ)),
        probe.COMMAND_STDOUT_BYTES,
        probe.COMMAND_STDERR_BYTES,
        60,
    )
    stdout = captured.get("stdout_bytes")
    complete = (
        captured.get("accepted") is True
        and captured.get("truncated") is False
        and captured.get("late") is False
        and type(captured.get("exit")) is int
        and isinstance(stdout, (bytes, bytearray))
    )
    return {
        "exit": captured.get("exit"),
        "stdout": bytes(stdout) if complete else b"",
        "complete": complete,
    }


def _inside_complete(launched):
    if not isinstance(launched, dict):
        return False
    if launched.get("truncated") is True or launched.get("wait_result") != "exited":
        return False
    return type(launched.get("exit")) is int and isinstance(launched.get("stdout"), (bytes, bytearray))


def _apply_launch_error(record, exc):
    record["outcome"] = layout.OUTCOME_SETUP
    record["reason"] = str(exc)[:HEAD]
    launch = getattr(exc, "launch", None)
    if isinstance(launch, dict):
        record["child_exit"] = launch.get("exit")
        record["stderr_head"] = _head(launch.get("stderr"))


def _before_resume(probe):
    def read(process):
        return probe.read_process_token(process, probe.INTERNET_CLIENT_SID)

    return read


def _run_control(probe, profile_sid, argv, state):
    record = _blank_launch()
    record["argv"] = list(argv)
    record["grants"] = []
    try:
        launched = probe.launch_in_profile(
            profile_sid,
            None,
            argv,
            probe.child_environment(os.environ),
            60,
            state["acquired"],
            label="control",
            before_resume=_before_resume(probe),
        )
        record["stdout_head"] = _head(launched.get("stdout"))
        record["child_exit"] = _child_exit(launched)
        record["stderr_head"] = _head(launched.get("stderr"))
        record["token"] = launched.get("token")
        outcome, reason = layout.classify_control(
            _inside_complete(launched), launched.get("exit"), True
        )
        record["outcome"] = outcome
        record["reason"] = reason
    except Exception as exc:
        launch = getattr(exc, "launch", None)
        if isinstance(launch, dict) and launch.get("create_process") is False:
            record["child_exit"] = launch.get("exit")
            record["stderr_head"] = _head(launch.get("stderr"))
            outcome, reason = layout.classify_control(False, None, False)
            record["outcome"] = outcome
            record["reason"] = reason
        else:
            _apply_launch_error(record, exc)
    return record


def _run_probe(probe, profile_sid, argv, state):
    record = _blank_launch()
    record["argv"] = argv
    try:
        launched = probe.launch_in_profile(
            profile_sid,
            None,
            argv,
            probe.child_environment(os.environ),
            30,
            state["acquired"],
            label="probe",
            before_resume=_before_resume(probe),
        )
        record["stdout_head"] = _head(launched.get("stdout"))
        record["child_exit"] = _child_exit(launched)
        record["stderr_head"] = _head(launched.get("stderr"))
        record["token"] = launched.get("token")
        outcome, reason = layout.classify_probe_outcome(launched.get("stdout"))
        record["outcome"] = outcome
        record["reason"] = reason
    except Exception as exc:
        _apply_launch_error(record, exc)
    return record


def _run_verifier(probe, profile_sid, argv, state):
    record = _blank_launch()
    record["argv"] = list(argv)
    record["compared"] = None
    outside = None
    try:
        outside = _outside_capture(probe, argv)
        launched = probe.launch_in_profile(
            profile_sid,
            None,
            argv,
            probe.child_environment(os.environ),
            60,
            state["acquired"],
            label="verifier",
            before_resume=_before_resume(probe),
        )
        record["child_exit"] = _child_exit(launched)
        record["stderr_head"] = _head(launched.get("stderr"))
        record["stdout_head"] = _head(launched.get("stdout"))
        record["token"] = launched.get("token")
        outcome, reason, compared = layout.classify_verifier_outcome(
            outside["exit"],
            outside["stdout"],
            outside["complete"],
            launched.get("exit"),
            launched.get("stdout") if _inside_complete(launched) else b"",
            _inside_complete(launched),
            launched.get("token"),
        )
        record["outcome"] = outcome
        record["reason"] = reason
        record["compared"] = compared
    except Exception as exc:
        _apply_launch_error(record, exc)
        if outside is not None and record.get("compared") is None:
            _outcome, fallback, compared = layout.classify_verifier_outcome(
                outside["exit"],
                outside["stdout"],
                outside["complete"],
                None,
                b"",
                False,
            )
            record["compared"] = compared
            if record["reason"] == "not recorded":
                record["reason"] = fallback
    return record


def main(argv):
    if "--self-test" in argv:
        return layout.self_test()
    import windows_appcontainer_probe as probe

    out = Path("results/windows-offline-layout.json").resolve()
    out.parent.mkdir(parents=True, exist_ok=True)
    document = {
        "launches": {
            "control": _blank_launch(),
            "probe": _blank_launch(),
            "verifier": _blank_launch(),
        },
        "grants": None,
        "pre_grant": None,
        "cleanup": None,
        "release_tag": layout.LAYOUT_RELEASE_TAG,
        "bundle_source": layout.BUNDLE_SOURCE,
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
        if not script.is_file():
            raise RuntimeError("missing file " + str(script))
        source = layout.place_bundle(bundle)
        if source.resolve() == bundle.resolve():
            raise RuntimeError("refusing to verify the checkout fixture in place")
        document["bundle_sha256"] = _sha256(bundle)
        preflight = layout.acquire_published_cli(probe, layout.acquire_work_dir(verifier), verifier)
        document["preflight"] = {
            "verified": preflight.get("verified") is True,
            "archive_digest_ok": preflight.get("archive_digest_ok") is True,
        }
        document["verifier_sha256"] = _sha256(verifier)
        grants = layout.layout_grant_paths(sys.base_prefix, script, bundle, verifier)
        if layout.grants_overlap(grants):
            raise RuntimeError("grant directories overlap")
        os.chdir(bundle.parent)
        profile = probe.create_profile_once()
        state["profile"] = profile
        document["profile_sid"] = profile["sid"]
        state["grants"] = []
        document["grants"] = state["grants"]
        listener = _ReadyListener()
        document["cwd"] = str(Path.cwd())
        document["base_prefix"] = sys.base_prefix
        document["executable"] = sys.executable
        document["script_sha256"] = _sha256(script)
        document["script_blob"] = os.environ.get("LAYOUT_SCRIPT_BLOB")
        probe_cmd = layout.probe_argv(sys.executable, script, "127.0.0.1", listener.port)
        verifier_cmd = layout.verifier_argv(verifier, bundle)

        def read_acl(grant_paths, container_sid):
            recorded = layout.pre_grant_record(
                grant_paths, container_sid, lambda path: probe._icacls([str(path)])
            )
            document["pre_grant"] = recorded
            return recorded

        def launch(argv, grant_list):
            if not grant_list:
                record = _run_control(probe, profile["sid"], argv, state)
                document["launches"]["control"] = record
                return record
            if "--probe" in argv:
                record = _run_probe(probe, profile["sid"], argv, state)
                document["launches"]["probe"] = record
                return record
            record = _run_verifier(probe, profile["sid"], argv, state)
            document["launches"]["verifier"] = record
            return record

        def grant(sid, grant_paths):
            probe.grant_paths(sid, grant_paths, state["grants"])

        layout.layout_sequence(
            read_acl,
            launch,
            grant,
            probe_cmd,
            verifier_cmd,
            grants,
            profile["sid"],
        )
    except Exception as exc:
        _fail_unrecorded(document, exc)
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
