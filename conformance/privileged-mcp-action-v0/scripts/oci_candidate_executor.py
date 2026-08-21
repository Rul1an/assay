#!/usr/bin/env python3
"""Bounded OCI candidate executor. Isolation limits are defense in depth.

Selects an image only by registry `implementation_id`. Constructs one create
argv, then runs pull -> inspect -> create -> bounded start/attach -> inspect
-> force-remove. Timeout, OOM, overflow, and pull/create/start/cleanup
failures are named execution states, never agreement.
"""

from __future__ import annotations

import argparse
import functools
import hashlib
import json
import os
import shutil
import sys
import tarfile
import tempfile
import uuid
from dataclasses import dataclass
from pathlib import Path
from typing import Any, Callable

sys.path.insert(0, str(Path(__file__).resolve().parent))
from bounded_process import ProcessLimitError, run_bounded  # noqa: E402
import capture_candidate  # noqa: E402
from capture_candidate import CandidateError, HarnessError  # noqa: E402
from capture_format import (  # noqa: E402
    STATE_CANDIDATE_ERROR,
    STATE_CAPTURE_ERROR,
    bound_error,
    observe_error,
    validate_capture,
)
from strict_json import load_strict_object  # noqa: E402

sys.path.insert(0, str(Path(__file__).resolve().parents[2]))
from implementations import (  # noqa: E402
    ImplementationRegistryError,
    load_implementations,
    validate_image_reference,
)

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "adequacy"))
import published_rows  # noqa: E402

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
STAGED_BUNDLE_NAME = "opaque.bundle"
STDOUT_LIMIT = 64 * 1024
STDERR_LIMIT = 64 * 1024
MAX_BUNDLE_BYTES = 16 * 1024 * 1024
EXECUTION_SCHEMA = "assay.privileged_mcp_action.oci_execution.v0"
EXECUTION_DOCUMENT_NAME = "oci-execution.json"
CANDIDATE_STDOUT_NAME = "candidate.stdout"
CANDIDATE_STDERR_NAME = "candidate.stderr"
HANDOFF_TEMP_PREFIX = ".assay-oci-handoff-"
CAPTURE_TEMP_PREFIX = ".assay-oci-capture-"
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


DOCKER_LIFECYCLE_ERRORS = (DockerCommandError, ProcessLimitError, OSError)


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


def _content_digest(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def resolve_docker_executable() -> Path:
    found = shutil.which("docker")
    if found is None:
        raise DockerCommandError("docker executable not found")
    return Path(found).resolve()


def resolve_local_docker_host() -> str | None:
    for socket in (Path("/var/run/docker.sock"), Path.home() / ".docker/run/docker.sock"):
        if socket.exists():
            return "unix://%s" % socket
    return None


def fresh_docker_env(parent: Path) -> tuple[dict[str, str], Path]:
    config_dir = Path(tempfile.mkdtemp(prefix="assay-oci-docker-config-", dir=parent))
    home = Path(parent) / "empty-home"
    home.mkdir(exist_ok=True)
    env = {
        "PATH": str(resolve_docker_executable().parent),
        "DOCKER_CONFIG": str(config_dir),
        "HOME": str(home),
        "TMPDIR": str(parent),
    }
    host = resolve_local_docker_host()
    if host is not None:
        env["DOCKER_HOST"] = host
    return env, config_dir


def wrap_docker_command(argv: list[str], env: dict[str, str]) -> list[str]:
    if not argv or argv[0] != "docker":
        raise DockerCommandError("docker argv must start with docker")
    assignments = ["%s=%s" % (key, env[key]) for key in sorted(env)]
    return ["env", "-i", *assignments, str(resolve_docker_executable()), *argv[1:]]


def implementation_from_registry(
    implementation_id: str, registry_path: Path | None = None
) -> dict[str, Any]:
    document = load_implementations(registry_path)
    for row in document["implementations"]:
        if row["id"] == implementation_id:
            validate_image_reference(row["image"])
            return row
    raise ImplementationRegistryError("unknown implementation id: %s" % implementation_id)


def identity_from_registry_row(row: dict[str, Any]) -> dict[str, Any]:
    return {
        "id": row["id"],
        "image": row["image"],
        "name": row["name"],
        "version": None,
        "source": row["source"],
        "commit": row["commit"],
        "reproduction_mode": row["reproduction_mode"],
    }


def reject_declared_volumes(image_inspect: dict[str, Any]) -> None:
    volumes = (image_inspect.get("Config") or {}).get("Volumes")
    if volumes:
        raise VolumeDeclarationError("image declares volume(s): %s" % sorted(volumes))


def stage_opaque_bundle(source: Path, staging_dir: Path) -> Path:
    staging_dir.mkdir(parents=True, exist_ok=True)
    data = published_rows.read_regular_file(Path(source), limit=MAX_BUNDLE_BYTES)
    staged = Path(staging_dir) / STAGED_BUNDLE_NAME
    staged.write_bytes(data)
    staged.chmod(0o400)
    return staged.resolve()


def build_container_create_argv(
    *,
    image: str,
    bundle_path: Path,
    container_name: str,
    staging_dir: Path,
    command: tuple[str, ...] = (),
) -> list[str]:
    validate_image_reference(image)
    staged = stage_opaque_bundle(bundle_path, staging_dir)
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
        f"type=bind,src={staged},dst={BUNDLE_DEST},ro=true",
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


def digest_image_refs(argv: list[str]) -> list[str]:
    refs: list[str] = []
    for item in argv:
        try:
            validate_image_reference(item)
        except ImplementationRegistryError:
            continue
        refs.append(item)
    return refs


def assert_wrapped_keeps_digest_refs(argv: list[str], wrapped: list[str]) -> None:
    for ref in digest_image_refs(argv):
        if ref not in wrapped:
            raise DockerCommandError("digest-qualified image was stripped before docker")


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
    wrapped = wrap_docker_command(argv, env)
    assert_wrapped_keeps_digest_refs(argv, wrapped)
    result = run_bounded(
        wrapped,
        timeout_seconds=timeout_seconds,
        stdout_limit=stdout_limit,
        stderr_limit=stderr_limit,
    )
    if result.returncode != 0 and not allow_nonzero:
        detail = result.stderr.decode("utf-8", "replace").strip() or f"exit {result.returncode}"
        raise DockerCommandError(detail)
    return BoundedDockerResult(result.returncode, result.stdout, result.stderr)


def _parse_inspect(payload: bytes) -> dict[str, Any]:
    try:
        document = json.loads(payload.decode("utf-8"))
    except json.JSONDecodeError as exc:
        raise DockerCommandError("docker inspect did not return an object") from exc
    if not isinstance(document, list) or not document or not isinstance(document[0], dict):
        raise DockerCommandError("docker inspect did not return an object")
    return document[0]


def _limit_state(error: ProcessLimitError) -> str:
    message = str(error)
    if "timed out" in message:
        return STATE_TIMEOUT
    if "exceeded" in message:
        return STATE_OUTPUT_OVERFLOW
    return STATE_START_FAILURE


def resolved_implementation_row(
    implementation_id: str,
    *,
    implementation: dict[str, Any] | None = None,
    registry_path: Path | None = None,
) -> dict[str, Any]:
    if implementation is not None:
        if implementation.get("id") != implementation_id:
            raise ImplementationRegistryError(
                "implementation id does not match the resolved row"
            )
        validate_image_reference(implementation["image"])
        return implementation
    return implementation_from_registry(implementation_id, registry_path)


def execute_candidate(
    *,
    implementation_id: str,
    bundle_path: Path,
    registry_path: Path | None = None,
    implementation: dict[str, Any] | None = None,
    timeout_seconds: int = 30,
    command: tuple[str, ...] = (),
    docker_runner: DockerRunner | None = None,
) -> OciExecution:
    runner = docker_runner or run_docker
    row = resolved_implementation_row(
        implementation_id, implementation=implementation, registry_path=registry_path
    )
    image = row["image"]
    state: str | None = None
    exit_code: int | None = None
    stdout = b""
    stderr = b""
    error = ""
    container_id: str | None = None
    container_name: str | None = None
    with tempfile.TemporaryDirectory(prefix="assay-oci-exec-") as raw:
        root = Path(raw)
        try:
            env, _config_dir = fresh_docker_env(root)
        except DOCKER_LIFECYCLE_ERRORS as exc:
            return OciExecution(
                STATE_PULL_FAILURE, implementation_id, image, None, b"", b"", str(exc)
            )
        staging_dir = root / "stage"
        try:
            try:
                runner(
                    ["docker", "pull", "--platform", PLATFORM, image],
                    env=env,
                    timeout_seconds=60,
                )
            except DOCKER_LIFECYCLE_ERRORS as exc:
                return OciExecution(
                    STATE_PULL_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            try:
                inspected = runner(["docker", "image", "inspect", image], env=env)
                reject_declared_volumes(_parse_inspect(inspected.stdout))
            except VolumeDeclarationError as exc:
                return OciExecution(
                    STATE_CREATE_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            except DOCKER_LIFECYCLE_ERRORS as exc:
                return OciExecution(
                    STATE_CREATE_FAILURE, implementation_id, image, None, b"", b"", str(exc)
                )
            container_name = f"assay-oci-{os.getpid()}-{uuid.uuid4().hex[:8]}"
            try:
                try:
                    created = runner(
                        build_container_create_argv(
                            image=image,
                            bundle_path=bundle_path,
                            container_name=container_name,
                            staging_dir=staging_dir,
                            command=command,
                        ),
                        env=env,
                    )
                except ValueError as exc:
                    raise DockerCommandError(str(exc)) from exc
                container_id = created.stdout.decode("utf-8", "replace").strip()
                if not container_id:
                    raise DockerCommandError("docker create returned no container id")
            except DOCKER_LIFECYCLE_ERRORS as exc:
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
            except DOCKER_LIFECYCLE_ERRORS as exc:
                if isinstance(exc, ProcessLimitError):
                    state = _limit_state(exc)
                else:
                    state = STATE_START_FAILURE
                error = str(exc)
            try:
                container = _parse_inspect(
                    runner(["docker", "inspect", container_id], env=env).stdout
                )
                raw_exit = (container.get("State") or {}).get("ExitCode")
                if isinstance(raw_exit, int):
                    exit_code = raw_exit
                    if (container.get("State") or {}).get("OOMKilled"):
                        state = STATE_OOM
                        error = error or "container OOMKilled"
                    if state is None:
                        state = STATE_COMPLETED
                else:
                    exit_code = None
                    if state is None:
                        state = STATE_START_FAILURE
                        error = "container exit code is unavailable"
            except DOCKER_LIFECYCLE_ERRORS as exc:
                if state is None:
                    state = STATE_START_FAILURE
                    error = str(exc)
        finally:
            target = container_id or container_name
            if target:
                try:
                    runner(
                        ["docker", "rm", "--force", "--volumes", target],
                        env=env,
                    )
                except DOCKER_LIFECYCLE_ERRORS as exc:
                    if state in CANDIDATE_LIMIT_STATES:
                        error = bound_error("%s; cleanup: %s" % (error or state, exc))
                    else:
                        state = STATE_CLEANUP_FAILURE
                        error = str(exc)
    if state is None:
        state = STATE_START_FAILURE
        error = error or "execution produced no state"
    return OciExecution(state, implementation_id, image, exit_code, stdout, stderr, error)


def write_handoff(output_dir: Path, result: OciExecution) -> None:
    output_dir = Path(output_dir)
    if output_dir.is_symlink() or output_dir.exists():
        raise ValueError("handoff destination must not already exist")
    if len(result.stdout) > STDOUT_LIMIT or len(result.stderr) > STDERR_LIMIT:
        raise ValueError("candidate output exceeds its byte ceiling")
    parent = output_dir.parent
    parent.mkdir(parents=True, exist_ok=True)
    tmp = Path(tempfile.mkdtemp(prefix=HANDOFF_TEMP_PREFIX, dir=str(parent)))
    try:
        document = {
            "schema": EXECUTION_SCHEMA,
            "implementation_id": result.implementation_id,
            "image": result.image,
            "execution_state": result.state,
            "exit_code": result.exit_code,
            "error": bound_error(result.error) if result.error else "",
            "oci_executor_non_claims": list(OCI_EXECUTOR_NON_CLAIMS),
            "candidate_output": {
                "stdout": CANDIDATE_STDOUT_NAME,
                "stderr": CANDIDATE_STDERR_NAME,
                "stdout_sha256": _content_digest(result.stdout),
                "stderr_sha256": _content_digest(result.stderr),
                "stdout_bytes": len(result.stdout),
                "stderr_bytes": len(result.stderr),
            },
        }
        payload = (json.dumps(document, indent=2, sort_keys=True) + "\n").encode("utf-8")
        (tmp / CANDIDATE_STDOUT_NAME).write_bytes(result.stdout)
        (tmp / CANDIDATE_STDERR_NAME).write_bytes(result.stderr)
        (tmp / EXECUTION_DOCUMENT_NAME).write_bytes(payload)
        load_strict_object(
            tmp / EXECUTION_DOCUMENT_NAME,
            label="oci execution",
            max_bytes=MAX_EXECUTION_BYTES,
            max_depth=MAX_EXECUTION_DEPTH,
        )
        if output_dir.is_symlink() or output_dir.exists():
            raise ValueError("handoff destination must not already exist")
        os.rename(tmp, output_dir)
    except Exception:
        shutil.rmtree(tmp, ignore_errors=True)
        raise


def oci_entrypoint_command(*, implementation_id: str) -> list[str]:
    return [
        sys.executable,
        str(Path(__file__).resolve()),
        "--implementation-id",
        implementation_id,
    ]


def parse_oci_command(command: list[str]) -> argparse.Namespace:
    argv = list(command)
    if argv and Path(argv[0]).name.startswith("python"):
        argv = argv[1:]
    if argv and argv[0].endswith("oci_candidate_executor.py"):
        argv = argv[1:]
    return parse_args(argv)


def run_oci_candidate(
    command: list[str],
    bundle: Path,
    timeout_seconds: int,
    *,
    docker_runner: DockerRunner | None = None,
    registry_path: Path | None = None,
    implementation: dict[str, Any] | None = None,
) -> dict[str, Any]:
    """Same shape as `capture_candidate.run_candidate` for the shared loop."""
    args = parse_oci_command(command)
    result = execute_candidate(
        implementation_id=args.implementation_id,
        bundle_path=bundle,
        registry_path=registry_path,
        implementation=implementation,
        timeout_seconds=timeout_seconds,
        docker_runner=docker_runner,
    )
    if result.state == STATE_COMPLETED:
        if result.exit_code is None:
            raise HarnessError("container exit code is unavailable")
        report = capture_candidate.parse_candidate_report(result.stdout)
        return {
            "exit_code": result.exit_code,
            "report": report,
            "stderr_present": bool(result.stderr),
        }
    if result.state in CANDIDATE_LIMIT_STATES:
        raise CandidateError(result.error or result.state)
    raise HarnessError(result.error or result.state)


def capture_oci_observations(
    pack: dict[str, Any],
    command: list[str],
    timeout_seconds: int,
    *,
    registry_path: Path | None = None,
    implementation: dict[str, Any] | None = None,
    docker_runner: DockerRunner | None = None,
) -> list[dict[str, Any]]:
    row = implementation
    if row is None:
        args = parse_oci_command(command)
        row = implementation_from_registry(args.implementation_id, registry_path)
    return capture_candidate.capture_observations(
        pack,
        command,
        timeout_seconds,
        candidate_runner=functools.partial(
            run_oci_candidate,
            implementation=row,
            docker_runner=docker_runner,
        ),
    )


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--implementation-id", required=True)
    parser.add_argument("--pack", type=Path)
    parser.add_argument("--bundle", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--timeout-seconds", type=int, default=30)
    return parser.parse_args(argv)


def write_validated_capture(
    pack_path: Path,
    implementation_id: str,
    output: Path,
    *,
    registry_path: Path | None = None,
    docker_runner: DockerRunner | None = None,
    timeout_seconds: int,
) -> None:
    row = implementation_from_registry(implementation_id, registry_path)
    command = oci_entrypoint_command(implementation_id=implementation_id)
    with tempfile.TemporaryDirectory() as tmp:
        pack, pack_digest = capture_candidate.load_pack_with_digest(pack_path, Path(tmp))
        observations = capture_oci_observations(
            pack,
            command,
            timeout_seconds,
            implementation=row,
            docker_runner=docker_runner,
        )
    capture = capture_candidate.build_capture(
        pack, pack_digest, observations, identity_from_registry_row(row)
    )
    validate_capture(capture)
    write_regular_file_atomically(
        output,
        (json.dumps(capture, indent=2, sort_keys=True) + "\n").encode("utf-8"),
    )
    print(f"captured {len(observations)} observations")


def write_regular_file_atomically(path: Path, data: bytes) -> None:
    parent = Path(path).parent
    parent.mkdir(parents=True, exist_ok=True)
    fd, tmp_name = tempfile.mkstemp(prefix=CAPTURE_TEMP_PREFIX, dir=str(parent))
    tmp = Path(tmp_name)
    try:
        written = 0
        while written < len(data):
            written += os.write(fd, data[written:])
        os.fsync(fd)
        os.close(fd)
        fd = -1
        os.replace(tmp, path)
    finally:
        if fd >= 0:
            os.close(fd)
        tmp.unlink(missing_ok=True)


def main(argv: list[str] | None = None) -> int:
    args = parse_args(argv)
    if args.timeout_seconds <= 0:
        print("timeout-seconds must be positive", file=sys.stderr)
        return 2
    if args.pack is not None and args.bundle is not None:
        print("use --pack or --bundle, not both", file=sys.stderr)
        return 2
    if args.pack is not None:
        if args.output is None:
            print("--output is required", file=sys.stderr)
            return 2
        try:
            write_validated_capture(
                args.pack,
                args.implementation_id,
                args.output,
                timeout_seconds=args.timeout_seconds,
            )
        except (
            ImplementationRegistryError,
            DockerCommandError,
            OSError,
            EOFError,
            ValueError,
            KeyError,
            RecursionError,
            json.JSONDecodeError,
            tarfile.TarError,
        ) as error:
            print(str(error), file=sys.stderr)
            return 2
        return 0
    if args.bundle is None:
        print("--pack or --bundle is required", file=sys.stderr)
        return 2
    try:
        result = execute_candidate(
            implementation_id=args.implementation_id,
            bundle_path=args.bundle,
            timeout_seconds=args.timeout_seconds,
        )
        if args.output is not None:
            write_handoff(args.output, result)
    except (ImplementationRegistryError, DockerCommandError, ValueError, OSError) as error:
        print(str(error), file=sys.stderr)
        return 2
    print(result.state)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
