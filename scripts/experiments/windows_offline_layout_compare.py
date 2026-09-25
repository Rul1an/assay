#!/usr/bin/env python3
"""What the layout harness compares, and the inputs that comparison uses.

The probe receipt is a shape check. A result of connected is the listener
handshake, so it is not a receipt. The verifier result is byte identity of
stdout plus exit 0 on both sides, and only when the harness read a
zero-capability AppContainer token from the suspended process. Neither one
classifies a network result beyond that handshake.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re
import shutil

PROBE_SCHEMA = "assay.offline_probe.v1"
PROBE_RESULTS = {"connected", "denied", "timeout", "error"}
MAX_RECEIPT_BYTES = 65536
OUTCOME_RECEIPT = "receipt"
OUTCOME_IDENTICAL = "verified-identical"
OUTCOME_SETUP = "setup failure"
COMPARED_RULE = "inside stdout bytes equal outside stdout bytes and both exits are 0"
# Published CLI this job measures. The push-triggered probe keeps its own pin.
LAYOUT_RELEASE_TAG = "v6.6.3"
# v0 ok-001 under --profile-version v1 is an invalid verdict (exit 2). The
# production argv selects v1, so the fixture is the v1 deny-bound accept.
BUNDLE_SOURCE = (
    "conformance/privileged-mcp-action-v1/vectors/"
    "ok-001-deny-bound-v1-observation.bundle.tar.gz"
)
REPO_ROOT = Path(__file__).resolve().parents[2]


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


def published_cli_names(repository, tag):
    """Same URL shape as windows_appcontainer_probe's release pin."""
    archive = "assay-" + tag + "-x86_64-pc-windows-msvc.zip"
    identity = (
        "https://github.com/"
        + repository
        + "/.github/workflows/release.yml@refs/tags/"
        + tag
    )
    return archive, identity


def verifier_argv(assay, bundle):
    """Golden-path order: bundle, then --format json, then --profile-version v1."""
    return [
        str(assay),
        "evidence",
        "verify-privileged-mcp-action",
        str(bundle),
        "--format",
        "json",
        "--profile-version",
        "v1",
    ]


def probe_argv(executable, script, host, port):
    return [str(executable), "-I", str(script), "--probe", str(host), str(port)]


def layout_grant_paths(base_prefix, script, bundle, verifier):
    """The four directories the design names. No file ACE and no ancestor."""
    return [
        (Path(base_prefix), True),
        (Path(script).parent, True),
        (Path(bundle).parent, True),
        (Path(verifier).parent, True),
    ]


def grants_overlap(paths):
    """True when a grant is not a directory or one grant contains another."""
    resolved = []
    for path, directory in paths:
        if directory is not True:
            return True
        resolved.append(Path(path).resolve())
    if len(set(resolved)) != len(resolved):
        return True
    for index, path in enumerate(resolved):
        for other_index, other in enumerate(resolved):
            if index != other_index and path in other.parents:
                return True
    return False


def acquire_work_dir(verifier):
    """Download tree beside install/bin, not inside a granted directory."""
    verifier = Path(verifier)
    if (
        verifier.name != "assay.exe"
        or verifier.parent.name != "bin"
        or verifier.parent.parent.name != "install"
    ):
        raise RuntimeError("verifier path is not install/bin/assay.exe")
    return verifier.parents[2] / "acquire"


def _real_exit(value):
    if type(value) is not int:
        return None
    return value


def _digest(stdout):
    if not isinstance(stdout, (bytes, bytearray)):
        return None, None
    data = bytes(stdout)
    return hashlib.sha256(data).hexdigest(), len(data)


def classify_verifier_outcome(
    outside_exit,
    outside_stdout,
    outside_complete,
    inside_exit,
    inside_stdout,
    inside_complete,
    containment=None,
):
    """Record the bytes the harness compared. Identical requires both exits 0 and a contained token."""
    compared = {
        "rule": COMPARED_RULE,
        "outside_exit": _real_exit(outside_exit),
        "outside_stdout_sha256": None,
        "outside_stdout_bytes": None,
        "inside_exit": _real_exit(inside_exit),
        "inside_stdout_sha256": None,
        "inside_stdout_bytes": None,
        "containment": _containment_view(containment),
    }
    if outside_complete:
        digest, size = _digest(outside_stdout)
        compared["outside_stdout_sha256"] = digest
        compared["outside_stdout_bytes"] = size
    if inside_complete:
        digest, size = _digest(inside_stdout)
        compared["inside_stdout_sha256"] = digest
        compared["inside_stdout_bytes"] = size
    if not outside_complete or compared["outside_stdout_sha256"] is None:
        return OUTCOME_SETUP, "outside verifier did not complete", compared
    if compared["outside_exit"] != 0:
        return OUTCOME_SETUP, "outside verifier exit was not 0", compared
    if not inside_complete or compared["inside_stdout_sha256"] is None:
        return OUTCOME_SETUP, "in-container verifier did not complete", compared
    if compared["inside_exit"] != 0:
        return "child_nonzero_exit", "in-container verifier exit was not 0", compared
    if bytes(outside_stdout) != bytes(inside_stdout):
        return "stdout_differs", "stdout bytes differ", compared
    if not token_contained(containment):
        if not isinstance(containment, dict):
            return OUTCOME_SETUP, "suspended token was not recorded", compared
        return OUTCOME_SETUP, "suspended token is not a zero-capability app container", compared
    return OUTCOME_IDENTICAL, None, compared


def place_bundle(destination):
    source = REPO_ROOT / BUNDLE_SOURCE
    if not source.is_file():
        raise RuntimeError("missing fixture " + BUNDLE_SOURCE)
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(source, destination)
    return source


def acquire_published_cli(probe, work, destination):
    """signature_preflight, pointed at this job's tag, then the probe pin restored."""
    archive, identity = published_cli_names(probe.REPOSITORY, LAYOUT_RELEASE_TAG)
    saved = (probe.RELEASE_TAG, probe.ARCHIVE_NAME, probe.CERTIFICATE_IDENTITY)
    probe.RELEASE_TAG = LAYOUT_RELEASE_TAG
    probe.ARCHIVE_NAME = archive
    probe.CERTIFICATE_IDENTITY = identity
    try:
        preflight = probe.signature_preflight(work)
    finally:
        probe.RELEASE_TAG, probe.ARCHIVE_NAME, probe.CERTIFICATE_IDENTITY = saved
    if preflight.get("verified") is not True or preflight.get("archive_digest_ok") is not True:
        raise RuntimeError("checksum preflight did not verify the archive")
    assay = Path(preflight["assay"])
    if not assay.is_file():
        raise RuntimeError("preflight returned no assay.exe")
    destination = Path(destination)
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(assay, destination)
    return preflight


def token_contained(token):
    """Harness-read token: AppContainer and an empty capability list."""
    if not isinstance(token, dict):
        return False
    return token.get("is_app_container") is True and token.get("capabilities") == []


def _containment_view(token):
    if not isinstance(token, dict):
        return None
    return {
        "is_app_container": token.get("is_app_container"),
        "capabilities": token.get("capabilities"),
    }


def _receipt_body(stdout):
    if not receipt_appeared(stdout):
        return None
    line = bytes(stdout).decode("utf-8").partition("\n")[0]
    return json.loads(line)


def classify_probe_outcome(stdout):
    """A receipt whose result is connected reached the harness listener."""
    receipt = _receipt_body(stdout)
    if receipt is None:
        return OUTCOME_SETUP, "receipt missing"
    if receipt.get("result") == "connected":
        return "connected", "probe connected to the harness listener"
    return OUTCOME_RECEIPT, None


def classify_control(complete, exit_code, create_process):
    """No-grant launch. A start failure or nonzero exit is the control result."""
    if create_process is False:
        return "child_nonzero_exit", "control process did not start"
    if complete is True and type(exit_code) is int and exit_code != 0:
        return "child_nonzero_exit", "control process exited nonzero"
    if complete is True and exit_code == 0:
        return "control-exited-0", "control process exited 0"
    return OUTCOME_SETUP, "control process did not complete"


def _sid_mentioned(text, sid):
    if not isinstance(sid, str) or not sid:
        return False
    pattern = r"(?<![A-Za-z0-9-])" + re.escape(sid) + r"(?![A-Za-z0-9-])"
    return re.search(pattern, text) is not None


def acl_flags(text, container_sid, readable):
    """Flag ALL APPLICATION PACKAGES and the container SID. Unreadable is unknown."""
    unknown = {"all_application_packages": None, "container_sid": None}
    if readable is not True or not isinstance(text, str):
        return unknown
    import windows_appcontainer_probe as probe

    packages = _sid_mentioned(text, probe.ALL_APPLICATION_PACKAGES) or (
        re.search(r"(?<![A-Za-z])ALL APPLICATION PACKAGES(?![A-Za-z])", text, re.IGNORECASE) is not None
    )
    return {
        "all_application_packages": packages,
        "container_sid": _sid_mentioned(text, container_sid),
    }


def pre_grant_subjects(paths):
    """Each grant directory, then its ancestors, once."""
    rows = []
    seen = set()
    for path, directory in paths:
        if directory is not True:
            continue
        current = Path(path).resolve()
        for index, item in enumerate((current, *current.parents)):
            key = str(item)
            if key in seen:
                continue
            seen.add(key)
            rows.append({"path": key, "ancestor": index != 0})
    return rows


def _acl_stdout(result):
    if not isinstance(result, dict):
        return None
    text = result.get("stdout")
    if isinstance(text, bytes):
        text = text.decode("utf-8", "replace")
    if not isinstance(text, str):
        return None
    return text


def pre_grant_record(paths, container_sid, read_acl):
    """One icacls result per grant directory and ancestor, before any grant."""
    rows = []
    for subject in pre_grant_subjects(paths):
        result = read_acl(subject["path"])
        text = _acl_stdout(result)
        readable = (
            isinstance(result, dict)
            and result.get("exit") == 0
            and result.get("truncated") is False
            and text is not None
        )
        flags = acl_flags(text if readable else None, container_sid, readable)
        rows.append(
            {
                "path": subject["path"],
                "ancestor": subject["ancestor"],
                "readable": readable,
                "all_application_packages": flags["all_application_packages"],
                "container_sid": flags["container_sid"],
            }
        )
    return rows


def layout_sequence(read_acl, launch, grant, probe_argv, verifier_argv, paths, sid):
    """Read ACLs, launch the verifier once with no grants, then grant, then measure."""
    pre_grant = read_acl(paths, sid)
    control = launch(list(verifier_argv), [])
    grant(sid, paths)
    probed = launch(list(probe_argv), list(paths))
    verified = launch(list(verifier_argv), list(paths))
    return {
        "pre_grant": pre_grant,
        "control": control,
        "probe": probed,
        "verifier": verified,
    }


def _expect(failures, cond, name):
    if not cond:
        failures.append(name)


def self_test():
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
        _expect(failures, receipt_appeared(line), result)
    _expect(
        failures,
        not receipt_appeared(b"")
        and not receipt_appeared(b"{}\n")
        and not receipt_appeared(line + b"extra\n")
        and not receipt_appeared(line.rstrip(b"\n")),
        "reject",
    )

    body = b'{"verdict":"valid"}\n'
    contained = {"is_app_container": True, "capabilities": []}
    outcome, reason, compared = classify_verifier_outcome(
        0, body, True, 0, body, True, contained
    )
    _expect(failures, outcome == OUTCOME_IDENTICAL and reason is None, "identical")
    _expect(failures, token_contained(contained), "token-contained")
    missing_token = classify_verifier_outcome(0, body, True, 0, body, True, None)
    _expect(failures, missing_token[0] != OUTCOME_IDENTICAL, "identical-needs-token")
    _expect(
        failures,
        missing_token[0] == OUTCOME_SETUP and missing_token[1] == "suspended token was not recorded",
        "missing-token-setup",
    )
    open_token = {"is_app_container": False, "capabilities": []}
    open_outcome, open_reason, open_compared = classify_verifier_outcome(
        0, body, True, 0, body, True, open_token
    )
    _expect(failures, open_outcome != OUTCOME_IDENTICAL, "open-token")
    _expect(
        failures,
        open_reason == "suspended token is not a zero-capability app container"
        and open_compared["containment"]["is_app_container"] is False,
        "open-token-recorded",
    )
    capable = {"is_app_container": True, "capabilities": ["internetClient"]}
    _expect(
        failures,
        classify_verifier_outcome(0, body, True, 0, body, True, capable)[0] != OUTCOME_IDENTICAL,
        "capability-blocks-identical",
    )
    _expect(failures, compared["outside_stdout_sha256"] == hashlib.sha256(body).hexdigest(), "digest")
    _expect(
        failures,
        compared["outside_stdout_bytes"] == len(body) and compared["rule"] == COMPARED_RULE,
        "compared",
    )
    raw = b"a\xff\n"
    _expect(
        failures,
        classify_verifier_outcome(0, raw, True, 0, raw, True)[2]["outside_stdout_sha256"]
        == hashlib.sha256(raw).hexdigest(),
        "raw-digest",
    )
    _expect(failures, classify_verifier_outcome(2, body, True, 2, body, True)[0] == OUTCOME_SETUP, "shared-nonzero")
    child_exit = classify_verifier_outcome(0, body, True, 1, body, True)
    _expect(failures, child_exit[0] == "child_nonzero_exit", "child-nonzero")
    _expect(failures, child_exit[0] != OUTCOME_SETUP, "child-nonzero-not-setup")
    _expect(failures, child_exit[1] == "in-container verifier exit was not 0", "inside-exit")
    differed = classify_verifier_outcome(0, body, True, 0, body + b" ", True)
    _expect(failures, differed[0] == "stdout_differs", "stdout-differs")
    _expect(failures, differed[0] != OUTCOME_SETUP, "stdout-differs-not-setup")
    _expect(failures, differed[1] == "stdout bytes differ", "differ")
    _expect(
        failures,
        classify_verifier_outcome(0, body, False, 0, body, True)[1] == "outside verifier did not complete",
        "outside-incomplete",
    )
    _expect(failures, classify_verifier_outcome(True, body, True, True, body, True)[0] == OUTCOME_SETUP, "bool-exit")

    argv = verifier_argv(Path("install/bin/assay.exe"), Path("results/produced.bundle.tar.gz"))
    _expect(
        failures,
        argv
        == [
            "install/bin/assay.exe",
            "evidence",
            "verify-privileged-mcp-action",
            "results/produced.bundle.tar.gz",
            "--format",
            "json",
            "--profile-version",
            "v1",
        ],
        "argv",
    )
    _expect(failures, probe_argv("python", "phase.py", "127.0.0.1", 9)[1:4] == ["-I", "phase.py", "--probe"], "probe-argv")
    connected_line = None
    denied_line = None
    for result, err in (
        ("timeout", "ETIMEDOUT"),
        ("connected", ""),
        ("denied", "EACCES"),
        ("error", ""),
    ):
        payload = {"errno": err, "result": result, "schema": PROBE_SCHEMA}
        parsed = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode() + b"\n"
        probe_outcome = classify_probe_outcome(parsed)
        if result == "connected":
            connected_line = parsed
            _expect(failures, probe_outcome[0] == "connected", "connected-outcome")
            _expect(failures, probe_outcome[0] != OUTCOME_RECEIPT, "connected-not-receipt")
            _expect(failures, probe_outcome[0] != OUTCOME_SETUP, "connected-not-setup")
        else:
            if result == "denied":
                denied_line = parsed
            _expect(failures, probe_outcome[0] == OUTCOME_RECEIPT and probe_outcome[1] is None, "receipt-" + result)
    _expect(failures, classify_probe_outcome(b"")[0] == OUTCOME_SETUP, "receipt-missing")
    _expect(failures, connected_line is not None and denied_line is not None, "probe-lines")
    control_fail = classify_control(True, 1, True)
    _expect(failures, control_fail[0] == "child_nonzero_exit", "control-nonzero")
    _expect(failures, control_fail[0] != OUTCOME_SETUP, "control-nonzero-not-setup")
    control_start = classify_control(False, None, False)
    _expect(failures, control_start[0] == "child_nonzero_exit", "control-did-not-start")
    _expect(failures, control_start[0] != OUTCOME_SETUP, "control-start-not-setup")
    _expect(failures, control_start[1] == "control process did not start", "control-start-reason")
    control_zero = classify_control(True, 0, True)
    _expect(
        failures,
        control_zero[0] not in (OUTCOME_IDENTICAL, OUTCOME_RECEIPT, OUTCOME_SETUP, "connected"),
        "control-zero-not-success",
    )
    _expect(failures, classify_control(False, None, None)[0] == OUTCOME_SETUP, "control-unmeasured")

    import tempfile

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        base = root / "python"
        script = root / "harness" / "scripts" / "ci" / "published_release_offline_phase.py"
        bundle = root / "results" / "produced.bundle.tar.gz"
        verifier = root / "install" / "bin" / "assay.exe"
        for path in (base, script.parent, bundle.parent, verifier.parent):
            path.mkdir(parents=True)
        grants = layout_grant_paths(base, script, bundle, verifier)
        expected = [base, script.parent, bundle.parent, verifier.parent]
        _expect(
            failures,
            [path for path, directory in grants] == expected and not grants_overlap(grants),
            "grants",
        )
        _expect(failures, grants_overlap(grants + [(root, True)]), "ancestor")
        _expect(failures, grants_overlap([(script, False)] + grants[1:]), "file-grant")
        _expect(failures, acquire_work_dir(verifier) == root / "acquire", "acquire-dir")
        source = place_bundle(bundle)
        _expect(
            failures,
            source == REPO_ROOT / BUNDLE_SOURCE and bundle.read_bytes() == source.read_bytes(),
            "bundle",
        )
        try:
            acquire_work_dir(root / "assay.exe")
            failures.append("verifier-path")
        except RuntimeError:
            pass

    import windows_appcontainer_probe as hosted

    archive, identity = published_cli_names(hosted.REPOSITORY, hosted.RELEASE_TAG)
    _expect(failures, archive == hosted.ARCHIVE_NAME and identity == hosted.CERTIFICATE_IDENTITY, "name-parity")
    pinned = (hosted.RELEASE_TAG, hosted.ARCHIVE_NAME, hosted.CERTIFICATE_IDENTITY)
    seen = {}

    def fake_preflight(work):
        seen["tag"] = hosted.RELEASE_TAG
        seen["archive"] = hosted.ARCHIVE_NAME
        assay = Path(work) / "unpacked" / "assay.exe"
        assay.parent.mkdir(parents=True, exist_ok=True)
        assay.write_bytes(b"MZ-fake")
        return {"verified": True, "archive_digest_ok": True, "assay": assay}

    original = hosted.signature_preflight
    hosted.signature_preflight = fake_preflight
    try:
        with tempfile.TemporaryDirectory() as tmp:
            dest = Path(tmp) / "install" / "bin" / "assay.exe"
            acquire_published_cli(hosted, Path(tmp) / "acquire", dest)
            _expect(failures, dest.read_bytes() == b"MZ-fake", "acquire-copy")
    finally:
        hosted.signature_preflight = original
    _expect(
        failures,
        seen.get("tag") == LAYOUT_RELEASE_TAG
        and str(seen.get("archive", "")).startswith("assay-" + LAYOUT_RELEASE_TAG + "-"),
        "acquire-tag",
    )
    _expect(
        failures,
        (hosted.RELEASE_TAG, hosted.ARCHIVE_NAME, hosted.CERTIFICATE_IDENTITY) == pinned,
        "pin-restored",
    )

    packages = "C:\\runner APPLICATION PACKAGE AUTHORITY\\ALL APPLICATION PACKAGES:(RX)\n"
    flagged = acl_flags(packages, "S-1-15-2-999", True)
    _expect(failures, flagged["all_application_packages"] is True, "packages-name")
    _expect(failures, flagged["container_sid"] is False, "packages-not-container")
    sid_text = "C:\\runner S-1-15-2-1:(OI)(CI)(RX)\n"
    _expect(failures, acl_flags(sid_text, "S-1-15-2-999", True)["all_application_packages"] is True, "packages-sid")
    longer = "C:\\runner S-1-15-2-1234:(RX)\n"
    longer_flags = acl_flags(longer, "S-1-15-2-1234", True)
    _expect(failures, longer_flags["all_application_packages"] is False, "packages-prefix")
    _expect(failures, longer_flags["container_sid"] is True, "container-sid")
    restricted = "C:\\runner ALL RESTRICTED APPLICATION PACKAGES:(RX)\n"
    _expect(
        failures,
        acl_flags(restricted, "S-1-15-2-9", True)["all_application_packages"] is False,
        "restricted-not-packages",
    )
    unreadable = acl_flags("", "S-1-15-2-9", False)
    _expect(
        failures,
        unreadable["all_application_packages"] is None and unreadable["container_sid"] is None,
        "unreadable-acl",
    )

    with tempfile.TemporaryDirectory() as tmp:
        root = Path(tmp)
        base = root / "python"
        script = root / "harness" / "scripts" / "ci" / "phase.py"
        bundle = root / "results" / "produced.bundle.tar.gz"
        verifier = root / "install" / "bin" / "assay.exe"
        for path in (base, script.parent, bundle.parent, verifier.parent):
            path.mkdir(parents=True)
        grants = layout_grant_paths(base, script, bundle, verifier)
        subjects = pre_grant_subjects(grants)
        subject_paths = [Path(row["path"]) for row in subjects]
        _expect(failures, len(subject_paths) == len(set(subject_paths)), "acl-unique")
        for path, directory in grants:
            resolved = Path(path).resolve()
            _expect(failures, any(row["path"] == str(resolved) and row["ancestor"] is False for row in subjects), "acl-grant")
            for ancestor in resolved.parents:
                _expect(
                    failures,
                    any(row["path"] == str(ancestor) and row["ancestor"] is True for row in subjects),
                    "acl-ancestor",
                )
        listings = {}

        def read_acl(path):
            text = listings.get(path, "C:\\clean BUILTIN\\Users:(RX)\n")
            return {"exit": 0, "stdout": text, "truncated": False}

        hit = str(Path(base).resolve().parent)
        listings[hit] = "ancestor APPLICATION PACKAGE AUTHORITY\\ALL APPLICATION PACKAGES:(RX)\n"
        recorded = pre_grant_record(grants, "S-1-15-2-999", read_acl)
        matched = [row for row in recorded if row["path"] == hit]
        _expect(
            failures,
            len(matched) == 1 and matched[0]["ancestor"] is True and matched[0]["all_application_packages"] is True,
            "pre-grant-ancestor",
        )
        denied_read = pre_grant_record(
            grants,
            "S-1-15-2-999",
            lambda _path: {"exit": 5, "stdout": "", "truncated": False},
        )
        _expect(
            failures,
            denied_read
            and all(row["readable"] is False and row["all_application_packages"] is None for row in denied_read),
            "pre-grant-unreadable",
        )

        order = []

        def launch(argv, grant_list):
            order.append(("launch", tuple(argv), len(grant_list)))
            if not grant_list:
                return {"outcome": "child_nonzero_exit", "grants": []}
            return {"outcome": "recorded", "grants": list(grant_list)}

        def grant(sid, grant_paths):
            order.append(("grant", sid, len(grant_paths)))

        def read_rows(paths, sid):
            order.append(("acl", sid, len(paths)))
            return [{"path": "recorded"}]

        probe_cmd = probe_argv("python", script, "127.0.0.1", 9)
        verifier_cmd = verifier_argv(verifier, bundle)
        sequenced = layout_sequence(read_rows, launch, grant, probe_cmd, verifier_cmd, grants, "S-1-15-2-42")
        _expect(failures, [step[0] for step in order] == ["acl", "launch", "grant", "launch", "launch"], "sequence")
        _expect(
            failures,
            len(order) > 1 and order[1][1] == tuple(verifier_cmd) and order[1][2] == 0,
            "control-no-grants",
        )
        _expect(failures, len(order) > 2 and order[2][1] == "S-1-15-2-42", "grant-after-control")
        control = sequenced.get("control") if isinstance(sequenced, dict) else None
        _expect(
            failures,
            isinstance(control, dict)
            and control.get("grants") == []
            and control.get("outcome") == "child_nonzero_exit",
            "control-recorded",
        )
        _expect(
            failures,
            isinstance(sequenced, dict) and sequenced.get("pre_grant") == [{"path": "recorded"}],
            "sequence-acl",
        )

    order = []

    class Kernel:
        def ResumeThread(self, thread):
            order.append(("resume", thread))
            return 1

        def TerminateProcess(self, process, code):
            order.append(("kill", process, code))
            return 1

    def reader(process):
        order.append(("read", process))
        return {"is_app_container": True, "capabilities": []}

    token = hosted.resume_suspended(Kernel(), "thread", "process", reader)
    _expect(failures, order == [("read", "process"), ("resume", "thread")], "resume-order")
    _expect(failures, token_contained(token), "resume-token")

    def boom(_process):
        order.append("boom")
        raise RuntimeError("token unread")

    order.clear()
    try:
        hosted.resume_or_terminate(Kernel(), "thread", "process", boom)
        failures.append("resume-raises")
    except RuntimeError:
        pass
    _expect(failures, order == ["boom", ("kill", "process", 1)], "resume-kills")
    if failures:
        print("self-test exit 1 " + ",".join(failures))
        return 1
    print("self-test exit 0")
    return 0
