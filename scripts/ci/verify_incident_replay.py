#!/usr/bin/env python3
"""Replay retained and freshly produced packages in offline runtime containers.

The caller supplies a trusted, already-built native Linux Assay binary. This is
an execution recipe, not another incident verifier or a hostile-code sandbox.
"""
import copy
from datetime import datetime, timedelta, timezone
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import signal
import tarfile
import subprocess
import sys
import tempfile
import uuid


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def main():
    binary, output = (Path(arg).resolve() for arg in sys.argv[1:])
    root = Path(__file__).resolve().parents[2]
    fixture = root / "crates/assay-evidence/tests/fixtures/incident_package"
    expected_hashes = json.loads((fixture / "combined_session_3345.sha256.json").read_text())
    for name, expected in expected_hashes.items():
        if digest(fixture / name) != expected:
            raise ValueError(f"retained fixture digest mismatch: {name}")
    images = re.findall(r"^FROM (\S+)$", (root / "docker/assay-mcp-server/Dockerfile").read_text(), re.M)
    if len(images) != 1:
        raise ValueError("expected one runtime FROM image")
    image = images[0]
    if not re.fullmatch(r"gcr.io/distroless/cc-debian13:nonroot@sha256:[0-9a-f]{64}", image):
        raise ValueError("runtime image must be pinned by digest")
    output.mkdir(parents=True, exist_ok=False)
    subprocess.run(["docker", "pull", image], check=True, timeout=180)
    image_info = json.loads(subprocess.check_output(
        ["docker", "image", "inspect", image], timeout=30))[0]
    receipt = {
        "recipe_checkout_sha": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True, timeout=30).strip(),
        "declared_binary_source_sha": os.environ.get("ASSAY_REPLAY_BINARY_SOURCE_SHA"),
        "binary_sha256": digest(binary), "fixture_sha256": expected_hashes,
        "runtime_image": image, "runtime_image_id": image_info["Id"],
        "platform": platform.platform(),
        "github_run_id": os.environ.get("GITHUB_RUN_ID"),
        "github_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT"),
        "runner_environment": os.environ.get("RUNNER_ENVIRONMENT"),
        "runner_image_version": os.environ.get("ImageVersion"),
        "non_claims": ["no independent verifier implementation",
                       "no leaf-verifier parity", "no activity completeness",
                       "no provider outcome", "explicit historical as_of replay"],
    }
    run_receiver(binary, fixture / "combined_session_3345.tar",
                 fixture / "combined_session_3345.context.json", output, receipt,
                 expected_report=json.loads((fixture / "combined_session_3345.report.json").read_bytes()))
    fresh_wrap(binary, root, output / "fresh", receipt)
    print("incident replay: retained and fresh packages verified offline")


def run_receiver(binary, package, context, output, provenance, *, reason=None, expected_report=None):
    """Only the three explicit files enter a new container for each case."""
    output.mkdir(parents=True, exist_ok=True)
    receipt = dict(provenance)
    name = "assay-incident-replay-" + uuid.uuid4().hex
    image = receipt["runtime_image"]
    expected_exit = 0 if reason is None else 2
    with tempfile.TemporaryDirectory(prefix="assay-incident-replay-") as tmp:
        payload = Path(tmp)
        payload.chmod(0o755)
        for source, target in [(binary, "assay"),
                               (package, "combined.tar"),
                               (context, "context.json")]:
            shutil.copyfile(source, payload / target)
            (payload / target).chmod(0o755 if target == "assay" else 0o444)
        receipt["payload_sha256"] = {p.name: digest(p) for p in sorted(payload.iterdir())}
        command = [
            "docker", "create", "--name", name, "--network=none",
            "--read-only", "--cap-drop=ALL", "--security-opt=no-new-privileges",
            "--user=65532:65532", "--memory=256m", "--pids-limit=64", "--cpus=1",
            "--tmpfs=/tmp:rw,noexec,nosuid,size=16m", "--env=HOME=/tmp",
            "--mount", f"type=bind,source={payload},target=/payload,readonly",
            "--entrypoint=/payload/assay", image,
            "evidence", "verify-incident-package", "/payload/combined.tar",
            "--context", "/payload/context.json", "--format", "json",
        ]
        receipt["command"] = command
        try:
            subprocess.run(command, capture_output=True, check=True, timeout=30)
            info = json.loads(subprocess.check_output(["docker", "inspect", name], timeout=30))[0]
            receipt["container_config"] = {key: info[key] for key in ("Id", "Image", "Config", "HostConfig", "Mounts")}
            if (info["HostConfig"]["NetworkMode"] != "none"
                    or not info["HostConfig"]["ReadonlyRootfs"]
                    or info["Config"]["User"] != "65532:65532"
                    or info["HostConfig"]["Privileged"]):
                raise ValueError("container isolation configuration differs")
            result = subprocess.run(["docker", "start", "-a", name], capture_output=True, timeout=60)
            state = json.loads(subprocess.check_output(["docker", "inspect", name], timeout=30))[0]["State"]
            receipt["container_state"] = state
            (output / "report.json").write_bytes(result.stdout)
            (output / "runtime.stderr").write_bytes(result.stderr)
            receipt["exit"] = result.returncode
            if result.returncode != expected_exit or state["ExitCode"] != expected_exit or state["OOMKilled"] or state["Running"]:
                raise ValueError("verifier container did not exit successfully")
            report = json.loads(result.stdout)
            if (report.get("schema") != "assay.incident.verify.v1"
                    or report.get("outcome") != ("package_verified" if reason is None else "package_refused")
                    or report.get("reason") != reason):
                raise ValueError("receiver did not return the expected verification outcome/reason")
            if reason is None and report.get("artifact_sha256") != receipt["payload_sha256"]["combined.tar"]:
                raise ValueError("receiver verified a different package")
            if expected_report is not None and report != expected_report:
                raise ValueError("fresh report differs from retained expected report")
            receipt["expected_result_matched"] = True
        except subprocess.TimeoutExpired as error:
            (output / "report.json").write_bytes(error.stdout or b"")
            (output / "runtime.stderr").write_bytes(error.stderr or b"")
            receipt["timeout"] = True
            raise
        finally:
            # Only this invocation's unique container; never prune other work.
            # Attempt removal before persisting the receipt: a full/unwritable
            # output directory must not leave a timed-out container running.
            try:
                cleanup = subprocess.run(
                    ["docker", "rm", "-f", name], capture_output=True, timeout=30)
                receipt["cleanup_exit"] = cleanup.returncode
                (output / "cleanup.stderr").write_bytes(cleanup.stderr)
                if cleanup.returncode != 0:
                    raise RuntimeError("owned container cleanup failed")
            except (OSError, subprocess.TimeoutExpired) as error:
                receipt["cleanup_error"] = type(error).__name__
                raise
            finally:
                (output / "receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    return report


def run_producer(command, cwd, output, label, stdin=None, *, timeout=30):
    """Bound the owned fixture process group and retain output even on timeout."""
    with (output / f"{label}.stdout").open("xb") as stdout, (output / f"{label}.stderr").open("xb") as stderr:
        process = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE,
                                   stdout=stdout, stderr=stderr, start_new_session=True)
        try:
            process.communicate(stdin, timeout=timeout)
            if process.returncode:
                raise subprocess.CalledProcessError(process.returncode, command)
        finally:
            # A fixture may leave descendants after the leader exits. Only this owned group.
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
            process.wait(timeout=5)
            (output / f"{label}.command.json").write_text(json.dumps({
                "argv": command, "exit": process.returncode,
                "non_claim": "process-group cleanup attempted; no hostile-code sandbox claim",
            }) + "\n")


def fresh_wrap(binary, root, output, provenance):
    output.mkdir()
    producer = output / "producer"
    producer.mkdir()
    (producer / ".assay/quickstart").mkdir(parents=True)
    fixture = root / "examples/mcp-quickstart/mock_server.py"
    policy = producer / "policy.yaml"
    policy.write_text("version: '2.0'\nname: offline-recipient\ntools:\n  allow: [read_file]\n  deny: [blocked]\n")
    log = producer / "decisions.ndjson"
    sidecar = producer / "receipts.ndjson"
    requests = [
        {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2024-11-05", "capabilities": {},
            "clientInfo": {"name": "offline-recipient", "version": "1"}}},
        {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
            "name": "read_file", "arguments": {"path": "synthetic.txt"}}},
        {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
            "name": "blocked", "arguments": {}}},
    ]
    request_bytes = b"".join((json.dumps(r) + "\n").encode() for r in requests)
    (producer / "requests.ndjson").write_bytes(request_bytes)
    run_producer([str(binary), "mcp", "wrap", "--policy", str(policy),
                  "--event-source", "assay://offline-recipient", "--decision-log", str(log),
                  "--request-receipts-out", str(sidecar),
                  "--", sys.executable, "-I", str(fixture)], producer, producer, "wrap", request_bytes)
    original_log_digest = digest(log)
    original_sidecar_digest = digest(sidecar)
    decisions = [json.loads(line)["data"] for line in log.read_bytes().splitlines()]
    if [r["decision"] for r in decisions] != ["allow", "deny"]:
        raise ValueError("fresh fixture did not retain the selected allow and deny")
    # Reuse the checked-in ContextInput envelope, never invent wire fields. This
    # recipient contract is frozen before prepare; negatives change only receiver copies.
    context = json.loads((root / "crates/assay-evidence/tests/fixtures/incident_package/combined_session_3345.context.json").read_text())
    now = datetime.now(timezone.utc).replace(microsecond=0)
    context.update(applicability=[], keys=[], expectations=[{
        "input_id": "wrap-decision-log", "sha256": original_log_digest, "require_disclosure": True}, {
        "input_id": "wrap-request-receipts", "sha256": original_sidecar_digest, "require_disclosure": False}],
        invocation={"tool": "offline-recipient-replay", "version": "1", "options": []},
        as_of=now.isoformat().replace("+00:00", "Z"),
        valid_from=now.isoformat().replace("+00:00", "Z"),
        valid_until=(now + timedelta(hours=1)).isoformat().replace("+00:00", "Z"))
    producer_context = producer / "context.json"
    producer_context.write_text(json.dumps(context) + "\n")
    package = output / "package.tar"
    run_producer([str(binary), "evidence", "prepare-incident-package", "--decision-log", str(log),
                  "--request-receipts", str(sidecar),
                  "--context", str(producer_context), "--out", str(package)], producer, producer, "prepare")
    prepared = json.loads((producer / "prepare.stdout").read_bytes())
    if prepared.get("counts", {}).get("units") != 2 or prepared.get("expectation") != "matched":
        raise ValueError("prepare did not examine the expected selected records")
    original_package_digest = digest(package)
    handoff = {"package_sha256": original_package_digest, "decision_log_sha256": original_log_digest,
               "request_sidecar_sha256": original_sidecar_digest,
               "fixture_sha256": digest(fixture), "policy_sha256": digest(policy),
               "context_sha256": digest(producer_context)}
    (output / "handoff.json").write_text(json.dumps(handoff) + "\n")
    fresh_provenance = dict(provenance, fixture_sha256={"mock_server.py": digest(fixture)},
                            non_claims=["no independent verifier implementation", "no activity completeness",
                                        "no provider outcome", "synthetic local tool; no hosted agent"])
    # Locate one object in our just-created, verified, bounded archive. No extraction.
    with tarfile.open(package, "r:") as archive:
        member = next((m for m in archive.getmembers() if m.name == "objects/" + original_log_digest), None)
        if member is None:
            raise ValueError("prepared package lacks the decision-log object")
        sidecar_member = archive.getmember("objects/" + original_sidecar_digest)
        assessment_member = archive.getmember("assessment.json")
        with archive.extractfile("assessment.json") as source:
            assessment = json.load(source)
    if any(s["observation"] != "unknown" for s in assessment["surfaces"]):
        raise ValueError("unexpected observation claim from a proxy-only fixture")
    original = package.read_bytes()
    for case, reason in [("valid", None), ("corrupt", "input_shape"), ("missing", "digest"),
                         ("wrong-expectation", "expectation_mismatch"), ("small-budget", "resource_selected"), ("assessment-corrupt", "digest"),
                         ("sidecar-corrupt", "input_shape"), ("sidecar-missing", "digest"),
                         ("sidecar-wrong-expectation", "expectation_mismatch")]:
        case_dir = output / case
        case_dir.mkdir()
        receiver_context = copy.deepcopy(context)
        selected = package
        if case in ("corrupt", "missing", "sidecar-corrupt", "sidecar-missing"):
            target = sidecar_member if case.startswith("sidecar-") else member
            selected = case_dir / "package.tar"
            if case in ("corrupt", "sidecar-corrupt"):
                changed = bytearray(original)
                changed[target.offset_data] ^= 1
                selected.write_bytes(changed)
            else:
                end = target.offset_data + ((target.size + 511) // 512) * 512
                selected.write_bytes(original[:target.offset] + original[end:])
        elif case == "assessment-corrupt":
            selected = case_dir / "package.tar"
            changed = bytearray(original)
            start = assessment_member.offset_data
            end = start + assessment_member.size
            offset = original.index(assessment["verification_context_sha256"].encode(), start, end)
            changed[offset] = ord("1") if changed[offset] == ord("0") else ord("0")
            selected.write_bytes(changed)
        elif case == "wrong-expectation":
            receiver_context["expectations"][0]["sha256"] = "0" * 64
        elif case == "sidecar-wrong-expectation":
            receiver_context["expectations"][1]["sha256"] = "0" * 64
        elif case == "small-budget":
            receiver_context["limits"]["outer_bytes"] = len(original) - 1
        context_path = case_dir / "context.json"
        context_path.write_text(json.dumps(receiver_context) + "\n")
        run_receiver(binary, selected, context_path, case_dir, fresh_provenance,
                     reason=reason, expected_report=prepared if case == "valid" else None)
    if (digest(log) != original_log_digest or digest(sidecar) != original_sidecar_digest
            or digest(package) != original_package_digest):
        raise ValueError("original evidence changed during replay")


if __name__ == "__main__":
    main()
