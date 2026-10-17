#!/usr/bin/env python3
"""Replay the retained #3345 package in a fresh, offline runtime container.

The caller supplies a trusted, already-built native Linux Assay binary. This is
an execution recipe, not another incident verifier or a hostile-code sandbox.
"""
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
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
    name = "assay-incident-replay-" + uuid.uuid4().hex
    receipt = {
        "recipe_checkout_sha": subprocess.check_output(
            ["git", "rev-parse", "HEAD"], cwd=root, text=True).strip(),
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
    with tempfile.TemporaryDirectory(prefix="assay-incident-replay-") as tmp:
        payload = Path(tmp)
        payload.chmod(0o755)
        for source, target in [(binary, "assay"),
                               (fixture / "combined_session_3345.tar", "combined.tar"),
                               (fixture / "combined_session_3345.context.json", "context.json")]:
            shutil.copyfile(source, payload / target)
            (payload / target).chmod(0o755 if target == "assay" else 0o444)
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
            result.check_returncode()
            if state["ExitCode"] != 0 or state["OOMKilled"] or state["Running"]:
                raise ValueError("verifier container did not exit successfully")
            report = json.loads(result.stdout)
            if report != json.loads((fixture / "combined_session_3345.report.json").read_bytes()):
                raise ValueError("fresh report differs from retained expected report")
            receipt["report_equal"] = True
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
    print("incident replay: exact package/context retained; fresh report matches")


if __name__ == "__main__":
    main()
