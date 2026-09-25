#!/usr/bin/env python3
"""What the layout harness compares, and the inputs that comparison uses.

The probe receipt is a shape check. The verifier result is byte identity of
stdout plus exit 0 on both sides. Neither one classifies a network result.
"""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
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
):
    """Record the bytes the harness compared. Identical requires both exits 0."""
    compared = {
        "rule": COMPARED_RULE,
        "outside_exit": _real_exit(outside_exit),
        "outside_stdout_sha256": None,
        "outside_stdout_bytes": None,
        "inside_exit": _real_exit(inside_exit),
        "inside_stdout_sha256": None,
        "inside_stdout_bytes": None,
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
        return OUTCOME_SETUP, "in-container verifier exit was not 0", compared
    if bytes(outside_stdout) != bytes(inside_stdout):
        return OUTCOME_SETUP, "stdout bytes differ", compared
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
    outcome, reason, compared = classify_verifier_outcome(0, body, True, 0, body, True)
    _expect(failures, outcome == OUTCOME_IDENTICAL and reason is None, "identical")
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
    _expect(
        failures,
        classify_verifier_outcome(0, body, True, 1, body, True)[1] == "in-container verifier exit was not 0",
        "inside-exit",
    )
    _expect(
        failures,
        classify_verifier_outcome(0, body, True, 0, body + b" ", True)[1] == "stdout bytes differ",
        "differ",
    )
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
    if failures:
        print("self-test exit 1 " + ",".join(failures))
        return 1
    print("self-test exit 0")
    return 0
