#!/usr/bin/env python3
"""Bounded OCI candidate executor. Isolation limits are defense in depth.

Selects an image only by registry `implementation_id`. Constructs one create
argv, then runs pull -> inspect -> create -> bounded start/attach -> inspect
-> force-remove. Timeout, OOM, overflow, and pull/create/start/cleanup
failures are named execution states, never agreement.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import tempfile
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_process import ProcessLimitError, run_bounded  # noqa: E402
from capture_format import (  # noqa: E402
    STATE_CANDIDATE_ERROR,
    STATE_CAPTURE_ERROR,
    observe_error,
)
from strict_json import load_strict_object  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from implementations import (  # noqa: E402
    ImplementationRegistryError,
    load_implementations,
    validate_image_reference,
)

PLATFORM = "linux/amd64"
RUNTIME_USER = "65532:65532"
CPUS = "0.50"
MEMORY = "256m"
MEMORY_SWAP = "256m"
PIDS_LIMIT = "64"
TMPFS_SPEC = "/tmp:rw,nosuid,nodev,noexec,size=16m"
LOG_MAX_SIZE = "64k"
LOG_MAX_FILE = "1"
BUNDLE_DEST = "/input/bundle.tar.gz"
STDOUT_LIMIT = 64 * 1024
STDERR_LIMIT = 64 * 1024
EXECUTION_SCHEMA = "assay.privileged_mcp_action.oci_execution.v0"
MAX_EXECUTION_BYTES = 16 * 1024
MAX_EXECUTION_DEPTH = 6

STATE_COMPLETED = "completed"
STATE_TIMEOUT = "timeout"
STATE_OOM = "oom"
STATE_OUTPUT_OVERFLOW = "output_overflow"
STATE_PULL_FAILURE = "pull_failure"
STATE_CREATE_FAILURE = "create_failure"
STATE_START_FAILURE = "start_failure"
STATE_CLEANUP_FAILURE = "cleanup_failure"
EXECUTION_STATES = (
    STATE_COMPLETED,
    STATE_TIMEOUT,
    STATE_OOM,
    STATE_OUTPUT_OVERFLOW,
    STATE_PULL_FAILURE,
    STATE_CREATE_FAILURE,
    STATE_START_FAILURE,
    STATE_CLEANUP_FAILURE,
)
CANDIDATE_LIMIT_STATES = (STATE_TIMEOUT, STATE_OOM, STATE_OUTPUT_OVERFLOW)
HARNESS_FAILURE_STATES = (
    STATE_PULL_FAILURE,
    STATE_CREATE_FAILURE,
    STATE_START_FAILURE,
    STATE_CLEANUP_FAILURE,
)

OCI_EXECUTOR_NON_CLAIMS = (
    "No container/kernel security, image authenticity, publisher identity, "
    "malware safety, supply-chain integrity, conformance, or cleanup "
    "guarantee after host/daemon/SIGKILL failure.",
)


class VolumeDeclarationError(ValueError):
    """The image declared a writable volume. Refuse to create."""


class DockerCommandError(RuntimeError):
    """A docker CLI invocation failed before a named limit applied."""


@dataclass(frozen=True)
class BoundedDockerResult:
    returncode: int
    stdout: bytes
    stderr: bytes


@dataclass(frozen=True)
class OciExecution:
    state: str
    implementation_id: str
    image: str
    exit_code: int | None
    stdout: bytes
    stderr: bytes
    error: str


DockerRunner = Callable[..., BoundedDockerResult]


def fresh_docker_env(parent: Path) -> tuple[dict[str, str], Path]:
    config_dir = Path(tempfile.mkdtemp(prefix="assay-oci-docker-config-", dir=parent))
    env = {key: value for key, value in os.environ.items() if key != "REGISTRY_AUTH_FILE"}
    env["DOCKER_CONFIG"] = str(config_dir)
    return env, config_dir


def implementation_from_registry(
    implementation_id: str, registry_path: Path | None = None
) -> dict[str, Any]:
    document = load_implementations(registry_path)
    for row in document["implementations"]:
        if row["id"] == implementation_id:
            validate_image_reference(row["image"])
            return row
    raise ImplementationRegistryError("unknown implementation id: %s" % implementation_id)


def reject_declared_volumes(image_inspect: dict[str, Any]) -> None:
    volumes = (image_inspect.get("Config") or {}).get("Volumes")
    if volumes:
        raise VolumeDeclarationError("image declares volume(s): %s" % sorted(volumes))


def build_container_create_argv(
    *,
    image: str,
    bundle_path: Path,
    container_name: str,
    command: tuple[str, ...] = (),
) -> list[str]:
    validate_image_reference(image)
    bundle = Path(bundle_path)
    if bundle.is_symlink() or not bundle.is_file():
        raise ValueError("bundle must be a regular file")
    bundle = bundle.resolve()
    return [
        "docker",
        "create",
        "--name",
        container_name,
        "--platform",
        PLATFORM,
        "--network",
        "none",
        "--read-only",
        "--user",
        RUNTIME_USER,
        "--cap-drop",
        "ALL",
        "--security-opt",
        "no-new-privileges:true",
        "--cpus",
        CPUS,
        "--memory",
        MEMORY,
        "--memory-swap",
        MEMORY_SWAP,
        "--pids-limit",
        PIDS_LIMIT,
        "--ipc",
        "none",
        "--tmpfs",
        TMPFS_SPEC,
        "--log-opt",
        f"max-size={LOG_MAX_SIZE}",
        "--log-opt",
        f"max-file={LOG_MAX_FILE}",
        "--restart",
        "no",
        "--mount",
        f"type=bind,src={bundle},dst={BUNDLE_DEST},ro=true",
        image,
        *command,
    ]


def observation_for(
    state: str,
    *,
    case_id: str,
    input_sha256: str,
    message: str,
) -> dict[str, Any]:
    if state == STATE_COMPLETED:
        raise ValueError("completed is not an error observation")
    if state in CANDIDATE_LIMIT_STATES:
        capture_state = STATE_CANDIDATE_ERROR
    elif state in HARNESS_FAILURE_STATES:
        capture_state = STATE_CAPTURE_ERROR
    else:
        raise ValueError("unknown execution state: %s" % state)
    return observe_error(case_id, input_sha256, capture_state, message)


def _wrap_docker(argv: list[str], env: dict[str, str]) -> list[str]:
    if not argv or argv[0] != "docker":
        raise DockerCommandError("docker argv must start with docker")
    return [
        "env",
        "-u",
        "REGISTRY_AUTH_FILE",
        f"DOCKER_CONFIG={env['DOCKER_CONFIG']}",
        *argv,
    ]


def run_docker(
    argv: list[str],
    *,
    env: dict[str, str] | None = None,
    timeout_seconds: int = 60,
    stdout_limit: int = 256 * 1024,
    stderr_limit: int = 256 * 1024,
    allow_nonzero: bool = False,
) -> BoundedDockerResult:
    if env is None:
        raise DockerCommandError("docker invocations require a fresh DOCKER_CONFIG")
    try:
        result = run_bounded(
            _wrap_docker(argv, env),
            timeout_seconds=timeout_seconds,
            stdout_limit=stdout_limit,
            stderr_limit=stderr_limit,
        )
    except ProcessLimitError as error:
        raise
    if result.returncode != 0 and not allow_nonzero:
        detail = result.stderr.decode("utf-8", "replace").strip() or f"exit {result.returncode}"
        raise DockerCommandError(detail)
    return BoundedDockerResult(result.returncode, result.stdout, result.stderr)


def local_image_docker_runner() -> DockerRunner:
    """Pull step records the canonical argv; the inert fixture is already local."""

    def runner(argv: list[str], **kwargs: Any) -> BoundedDockerResult:
        if len(argv) >= 2 and argv[1] == "pull":
            image = argv[-1]
            expected = ["docker", "pull", "--platform", PLATFORM, image]
            if argv != expected:
                raise DockerCommandError("pull argv drifted from the digest/platform contract")
            probe = run_bounded(
                ["docker", "image", "inspect", image],
                timeout_seconds=30,
                stdout_limit=1_000_000,
                stderr_limit=64 * 1024,
            )
            if probe.returncode != 0:
                raise DockerCommandError("local fixture image is missing")
            return BoundedDockerResult(0, b"", b"")
        return run_docker(argv, **kwargs)

    return runner


def _parse_inspect(payload: bytes) -> dict[str, Any]:
    document = json.loads(payload.decode("utf-8"))
    if not isinstance(document, list) or not document:
        raise DockerCommandError("docker inspect did not return an object")
    first = document[0]
    if not isinstance(first, dict):
        raise DockerCommandError("docker inspect did not return an object")
    return first


def _limit_state(error: ProcessLimitError) -> str:
    message = str(error)
    if "timed out" in message:
        return STATE_TIMEOUT
    if "exceeded" in message:
        return STATE_OUTPUT_OVERFLOW
    return STATE_START_FAILURE


def execute_candidate(
    *,
    implementation_id: str,
    bundle_path: Path,
    registry_path: Path | None = None,
    timeout_seconds: int = 30,
    command: tuple[str, ...] = (),
    docker_runner: DockerRunner | None = None,
) -> OciExecution:
    runner = docker_runner or run_docker
    row = implementation_from_registry(implementation_id, registry_path)
    image = row["image"]
    state: str | None = None
    exit_code: int | None = None
    stdout = b""
    stderr = b""
    error = ""
    container_id: str | None = None
    with tempfile.TemporaryDirectory(prefix="assay-oci-exec-") as raw:
        env, _config_dir = fresh_docker_env(Path(raw))
        try:
            try:
                runner(
                    ["docker", "pull", "--platform", PLATFORM, image],
                    env=env,
                    timeout_seconds=60,
                )
            except (DockerCommandError, ProcessLimitError, OSError) as exc:
                return OciExecution(
                    STATE_PULL_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            try:
                inspected = runner(["docker", "inspect", image], env=env)
                reject_declared_volumes(_parse_inspect(inspected.stdout))
            except VolumeDeclarationError as exc:
                return OciExecution(
                    STATE_CREATE_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            except (DockerCommandError, ProcessLimitError, json.JSONDecodeError) as exc:
                return OciExecution(
                    STATE_CREATE_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            name = f"assay-oci-{os.getpid()}-{uuid.uuid4().hex[:8]}"
            try:
                created = runner(
                    build_container_create_argv(
                        image=image,
                        bundle_path=bundle_path,
                        container_name=name,
                        command=command,
                    ),
                    env=env,
                )
                container_id = created.stdout.decode("utf-8", "replace").strip()
                if not container_id:
                    raise DockerCommandError("docker create returned no container id")
            except (DockerCommandError, ProcessLimitError, ValueError) as exc:
                return OciExecution(
                    STATE_CREATE_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            try:
                started = runner(
                    ["docker", "start", "-a", container_id],
                    env=env,
                    timeout_seconds=timeout_seconds,
                    stdout_limit=STDOUT_LIMIT,
                    stderr_limit=STDERR_LIMIT,
                    allow_nonzero=True,
                )
                stdout = started.stdout
                stderr = started.stderr
            except ProcessLimitError as exc:
                state = _limit_state(exc)
                error = str(exc)
            except DockerCommandError as exc:
                state = STATE_START_FAILURE
                error = str(exc)
            try:
                container = _parse_inspect(
                    runner(["docker", "inspect", container_id], env=env).stdout
                )
                exit_code = (container.get("State") or {}).get("ExitCode")
                if not isinstance(exit_code, int):
                    exit_code = None
                if (container.get("State") or {}).get("OOMKilled") and state not in (
                    STATE_TIMEOUT,
                    STATE_OUTPUT_OVERFLOW,
                ):
                    state = STATE_OOM
                    error = error or "container OOMKilled"
                if state is None:
                    state = STATE_COMPLETED
            except (DockerCommandError, ProcessLimitError, json.JSONDecodeError) as exc:
                if state is None:
                    state = STATE_START_FAILURE
                    error = str(exc)
        finally:
            if container_id:
                try:
                    runner(
                        ["docker", "rm", "--force", "--volumes", container_id],
                        env=env,
                    )
                except (DockerCommandError, ProcessLimitError) as exc:
                    state = STATE_CLEANUP_FAILURE
                    error = str(exc)
    if state is None:
        state = STATE_START_FAILURE
        error = error or "execution produced no state"
    return OciExecution(state, implementation_id, image, exit_code, stdout, stderr, error)


def write_execution(path: Path, result: OciExecution) -> None:
    document = {
        "schema": EXECUTION_SCHEMA,
        "implementation_id": result.implementation_id,
        "image": result.image,
        "execution_state": result.state,
        "exit_code": result.exit_code,
        "error": result.error,
        "oci_executor_non_claims": list(OCI_EXECUTOR_NON_CLAIMS),
    }
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    load_strict_object(
        path,
        label="oci execution",
        max_bytes=MAX_EXECUTION_BYTES,
        max_depth=MAX_EXECUTION_DEPTH,
    )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--implementation-id", required=True)
    parser.add_argument("--bundle", type=Path, required=True)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--timeout-seconds", type=int, default=30)
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.timeout_seconds <= 0:
        print("timeout-seconds must be positive", file=sys.stderr)
        return 2
    try:
        result = execute_candidate(
            implementation_id=args.implementation_id,
            bundle_path=args.bundle,
            timeout_seconds=args.timeout_seconds,
        )
    except (ImplementationRegistryError, ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 2
    write_execution(args.output, result)
    print(result.state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
