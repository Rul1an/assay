#!/usr/bin/env python3
"""Canonical bounded OCI executor contract (assay-tunnel-experiments #203).

    python3 -W error::ResourceWarning \\
        conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py

Argv pins and live `docker inspect` assertions are independent. Mutating the
canonical builder must break both, not a workflow comment.
"""

from __future__ import annotations

import ast
import functools
import hashlib
import inspect
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path
from typing import Any
from unittest import mock

CORPUS = Path(__file__).resolve().parents[1]
REPO = CORPUS.parents[1]
SCRIPTS = CORPUS / "scripts"
FIXTURE_C = Path(__file__).resolve().parent / "fixtures" / "oci-candidate.c"

sys.path.insert(0, str(SCRIPTS))
sys.path.insert(0, str(REPO / "conformance"))

try:
    import oci_candidate_executor as oci
except ModuleNotFoundError as exc:
    if getattr(exc, "name", None) != "oci_candidate_executor":
        raise
    oci = None

import capture_candidate
import implementations
from bounded_process import ProcessLimitError
from capture_format import (  # noqa: E402
    CAPTURE_SCHEMA,
    MAX_ERROR_CHARS,
    STATE_CANDIDATE_ERROR,
    STATE_CAPTURE_ERROR,
    bound_error,
    validate_capture,
)
from score_candidate import STATE_TO_STATUS

DIGEST_IMAGE = (
    "ghcr.io/example/checker@sha256:"
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
)
SWAPPED_IMAGE = (
    "ghcr.io/example/swapped@sha256:"
    "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
)
IMAGE_ID = "sha256:" + "cd" * 32
AGREEMENT = frozenset(
    {"match", "mismatch", "unproved", "execution_error", "harness_error"}
)
ISSUE_203_NON_CLAIMS = (
    "No container/kernel security, image authenticity, publisher identity, "
    "malware safety, supply-chain integrity, conformance, or cleanup "
    "guarantee after host/daemon/SIGKILL failure."
)


def _require() -> Any:
    if oci is None:
        raise AssertionError("oci_candidate_executor.py is missing")
    return oci


def _registry_doc(image: str, ident: str = "inert-fixture") -> dict[str, Any]:
    return {
        "schema": "assay.conformance.implementations.v0",
        "implementations": [
            {
                "id": ident,
                "name": "Inert fixture",
                "suite": "privileged-mcp-action-v0",
                "image": image,
                "source": "https://github.com/Rul1an/assay",
                "commit": "0123456789abcdef0123456789abcdef01234567",
                "language": "c",
                "reproduction_mode": "other_disclosed",
                "authorship": {"kind": "human"},
            }
        ],
    }


def _write_registry(directory: Path, image: str, ident: str = "inert-fixture") -> Path:
    path = directory / "implementations.json"
    path.write_text(json.dumps(_registry_doc(image, ident)), encoding="utf-8")
    return path


def _bundle(directory: Path) -> Path:
    path = directory / "opaque.bundle.tar.gz"
    path.write_bytes(b"opaque-bundle")
    return path


def _docker_kind(argv: list[str]) -> str:
    if len(argv) >= 3 and argv[1] == "image" and argv[2] == "inspect":
        return "image-inspect"
    return argv[1] if len(argv) > 1 else ""


def rewrite_fixture_image_argv(
    argv: list[str], *, registry_ref: str, local_ref: str
) -> list[str]:
    if not registry_ref or not local_ref or registry_ref == local_ref:
        raise ValueError("fixture registry ref and local ref must be distinct")
    if not (local_ref.startswith("sha256:") and len(local_ref) == 71):
        raise ValueError("local fixture ref must be an immutable image id")
    return [local_ref if item == registry_ref else item for item in argv]


def local_image_docker_runner(*, registry_ref: str, local_ref: str) -> Any:
    module = _require()
    if not (local_ref.startswith("sha256:") and len(local_ref) == 71):
        raise ValueError("local fixture ref must be an immutable image id")

    def runner(argv: list[str], **kwargs: Any) -> Any:
        env = kwargs.get("env")
        if env is None:
            raise module.DockerCommandError("docker invocations require a fresh DOCKER_CONFIG")
        if len(argv) >= 2 and argv[1] == "pull":
            expected = ["docker", "pull", "--platform", module.PLATFORM, registry_ref]
            if argv != expected:
                raise module.DockerCommandError("pull argv drifted from the digest/platform contract")
            probe = module.run_bounded(
                module.wrap_docker_command(
                    rewrite_fixture_image_argv(
                        ["docker", "image", "inspect", registry_ref],
                        registry_ref=registry_ref,
                        local_ref=local_ref,
                    ),
                    env,
                ),
                timeout_seconds=30,
                stdout_limit=1_000_000,
                stderr_limit=64 * 1024,
            )
            if probe.returncode != 0:
                raise module.DockerCommandError("local fixture image is missing")
            return module.BoundedDockerResult(0, b"", b"")
        return module.run_docker(
            rewrite_fixture_image_argv(argv, registry_ref=registry_ref, local_ref=local_ref),
            **kwargs,
        )

    return runner


def _inspect_doc(*, exit_code: Any = 0, oom: bool = False) -> dict[str, Any]:
    return {
        "Id": IMAGE_ID,
        "Config": {"Volumes": None, "User": "65532:65532"},
        "HostConfig": {"NetworkMode": "none"},
        "State": {"OOMKilled": oom, "ExitCode": exit_code, "Status": "exited"},
    }


def _argv(*, image: str = DIGEST_IMAGE, bundle: Path | None = None) -> list[str]:
    module = _require()
    with tempfile.TemporaryDirectory() as raw:
        root = Path(raw)
        path = bundle or _bundle(root)
        staging = root / "stage"
        staging.mkdir()
        return module.build_container_create_argv(
            image=image,
            bundle_path=path,
            container_name="assay-oci-test",
            staging_dir=staging,
        )


def _flag_value(argv: list[str], flag: str) -> str | None:
    for index, item in enumerate(argv):
        if item == flag and index + 1 < len(argv):
            return argv[index + 1]
    return None


def _has_flag(argv: list[str], flag: str) -> bool:
    return flag in argv


def assert_argv_network_none(argv: list[str]) -> None:
    if _flag_value(argv, "--network") != "none":
        raise AssertionError("argv lost --network none")


def assert_argv_runtime_user(argv: list[str]) -> None:
    if _flag_value(argv, "--user") != "65532:65532":
        raise AssertionError("argv lost fixed non-root user 65532:65532")


def assert_inspect_network_none(inspect_doc: dict[str, Any]) -> None:
    mode = inspect_doc.get("HostConfig", {}).get("NetworkMode")
    if mode != "none":
        raise AssertionError(f"inspect NetworkMode is {mode!r}, not none")


def assert_inspect_runtime_user(inspect_doc: dict[str, Any]) -> None:
    user = inspect_doc.get("Config", {}).get("User")
    if user != "65532:65532":
        raise AssertionError(f"inspect User is {user!r}, not 65532:65532")


VALUELESS_FLAGS = frozenset(("--read-only",))


def _drop_pair(argv: list[str], flag: str) -> list[str]:
    if flag in VALUELESS_FLAGS:
        return [item for item in argv if item != flag]
    out: list[str] = []
    skip = False
    for item in argv:
        if skip:
            skip = False
            continue
        if item == flag:
            skip = True
            continue
        out.append(item)
    return out


def _replace_value(argv: list[str], flag: str, value: str) -> list[str]:
    out = list(argv)
    for index, item in enumerate(out):
        if item == flag and index + 1 < len(out):
            out[index + 1] = value
            return out
    raise AssertionError(f"missing {flag}")


class FirstRedIndependence(unittest.TestCase):
    """#203 first RED: drop --network none or change the user; both checks fail."""

    def test_canonical_argv_pins_network_none(self) -> None:
        assert_argv_network_none(_argv())

    def test_canonical_argv_pins_non_root_user(self) -> None:
        assert_argv_runtime_user(_argv())

    def test_inspect_helper_is_independent_of_argv_text(self) -> None:
        assert_inspect_network_none({"HostConfig": {"NetworkMode": "none"}})
        assert_inspect_runtime_user({"Config": {"User": "65532:65532"}})
        with self.assertRaises(AssertionError):
            assert_inspect_network_none({"HostConfig": {"NetworkMode": "bridge"}})
        with self.assertRaises(AssertionError):
            assert_inspect_runtime_user({"Config": {"User": "0:0"}})

    def test_mutating_network_none_fails_argv_and_inspect_independently(self) -> None:
        mutated_argv = _drop_pair(_argv(), "--network")
        with self.assertRaises(AssertionError):
            assert_argv_network_none(mutated_argv)
        with self.assertRaises(AssertionError):
            assert_inspect_network_none({"HostConfig": {"NetworkMode": "bridge"}})

    def test_mutating_user_fails_argv_and_inspect_independently(self) -> None:
        mutated_argv = _replace_value(_argv(), "--user", "0:0")
        with self.assertRaises(AssertionError):
            assert_argv_runtime_user(mutated_argv)
        with self.assertRaises(AssertionError):
            assert_inspect_runtime_user({"Config": {"User": "0:0"}})


class ArgvContract(unittest.TestCase):
    def test_one_canonical_builder(self) -> None:
        module = _require()
        self.assertTrue(callable(module.build_container_create_argv))
        tree = ast.parse(Path(module.__file__).read_text(encoding="utf-8"))
        builders = [
            node.name
            for node in ast.walk(tree)
            if isinstance(node, ast.FunctionDef) and node.name.startswith("build_")
        ]
        self.assertEqual(builders, ["build_container_create_argv"])

    def test_builder_validates_image_with_registry_function(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaises(implementations.ImplementationRegistryError):
                module.build_container_create_argv(
                    image="ghcr.io/example/checker:latest",
                    bundle_path=Path(raw) / "missing",
                    container_name="x",
                    staging_dir=Path(raw) / "stage",
                )

    def test_argv_pins_every_isolation_bound(self) -> None:
        argv = _argv()
        self.assertEqual(argv[0], "docker")
        self.assertEqual(argv[1], "create")
        self.assertEqual(_flag_value(argv, "--platform"), "linux/amd64")
        self.assertEqual(_flag_value(argv, "--network"), "none")
        self.assertTrue(_has_flag(argv, "--read-only"))
        self.assertEqual(_flag_value(argv, "--user"), "65532:65532")
        self.assertEqual(_flag_value(argv, "--cap-drop"), "ALL")
        self.assertEqual(_flag_value(argv, "--security-opt"), "no-new-privileges:true")
        self.assertEqual(_flag_value(argv, "--cpus"), module_cpus())
        self.assertEqual(_flag_value(argv, "--memory"), module_memory())
        self.assertEqual(_flag_value(argv, "--memory-swap"), module_memory_swap())
        self.assertEqual(_flag_value(argv, "--pids-limit"), module_pids())
        self.assertEqual(_flag_value(argv, "--ipc"), "none")
        self.assertEqual(_flag_value(argv, "--restart"), "no")
        tmpfs = _flag_value(argv, "--tmpfs")
        self.assertIsNotNone(tmpfs)
        self.assertTrue(tmpfs.startswith("/tmp:"))
        self.assertIn("noexec", tmpfs)
        self.assertIn("size=", tmpfs)
        log_opts = [
            argv[index + 1]
            for index, item in enumerate(argv)
            if item == "--log-opt" and index + 1 < len(argv)
        ]
        self.assertTrue(any(opt.startswith("max-size=") for opt in log_opts))
        self.assertTrue(any(opt.startswith("max-file=") for opt in log_opts))
        self.assertIn(DIGEST_IMAGE, argv)
        joined = " ".join(argv)
        for forbidden in (
            "/var/run/docker.sock",
            "docker.sock",
            str(REPO),
            str(Path.home()),
            ".docker/config.json",
            "--privileged",
        ):
            self.assertNotIn(forbidden, joined)
        self.assertNotIn("--pid", argv)
        self.assertNotIn("--volume", argv)

    def test_mount_is_only_the_opaque_bundle_readonly(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            bundle = _bundle(Path(raw))
            staging = Path(raw) / "stage"
            argv = _require().build_container_create_argv(
                image=DIGEST_IMAGE,
                bundle_path=bundle,
                container_name="assay-oci-test",
                staging_dir=staging,
            )
        mounts = [
            argv[index + 1]
            for index, item in enumerate(argv)
            if item == "--mount" and index + 1 < len(argv)
        ]
        self.assertEqual(len(mounts), 1)
        mount = mounts[0]
        staged = (staging / "opaque.bundle").resolve()
        self.assertIn("type=bind", mount)
        self.assertIn(f"src={staged}", mount)
        self.assertNotIn(str(bundle.resolve()), mount)
        self.assertIn("dst=/input/bundle.tar.gz", mount)
        self.assertTrue("ro=true" in mount or "readonly=true" in mount)

    def test_directory_bundle_is_rejected(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            with self.assertRaises(ValueError):
                module.build_container_create_argv(
                    image=DIGEST_IMAGE,
                    bundle_path=Path(raw),
                    container_name="x",
                    staging_dir=Path(raw) / "stage",
                )


def module_cpus() -> str:
    return _require().CPUS


def module_memory() -> str:
    return _require().MEMORY


def module_memory_swap() -> str:
    return _require().MEMORY_SWAP


def module_pids() -> str:
    return _require().PIDS_LIMIT


class RegistrySelection(unittest.TestCase):
    def test_implementation_id_comes_from_registry_not_cli_image(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            row = module.implementation_from_registry("inert-fixture", registry)
        self.assertEqual(row["image"], DIGEST_IMAGE)
        self.assertEqual(row["id"], "inert-fixture")

    def test_unknown_id_is_rejected(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            with self.assertRaises(implementations.ImplementationRegistryError):
                module.implementation_from_registry("missing-id", registry)

    def test_checked_in_registry_has_no_direct_image_escape(self) -> None:
        module = _require()
        with self.assertRaises(implementations.ImplementationRegistryError):
            module.implementation_from_registry("inert-fixture")

    def test_cli_rejects_direct_image_input(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            output = Path(raw) / "out.json"
            bundle = _bundle(Path(raw))
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(module.__file__)),
                    "--implementation-id",
                    "inert-fixture",
                    "--implementation-image",
                    DIGEST_IMAGE,
                    "--bundle",
                    str(bundle),
                    "--output",
                    str(output),
                ],
                cwd=REPO,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unrecognized arguments", result.stderr)

    def test_public_cli_rejects_registry_override(self) -> None:
        module = _require()
        with self.assertRaises(SystemExit):
            module.parse_args(
                [
                    "--implementation-id",
                    "inert-fixture",
                    "--registry",
                    "/tmp/implementations.json",
                ]
            )
        self.assertNotIn("--registry", inspect.getsource(module.parse_args))
        self.assertNotIn("--registry", inspect.getsource(module.oci_entrypoint_command))
        self.assertNotIn("--registry", inspect.getsource(module.main))


class NamedExecutionStates(unittest.TestCase):
    def test_named_states_are_never_agreement(self) -> None:
        module = _require()
        expected = {
            module.STATE_COMPLETED,
            module.STATE_TIMEOUT,
            module.STATE_OOM,
            module.STATE_OUTPUT_OVERFLOW,
            module.STATE_PULL_FAILURE,
            module.STATE_CREATE_FAILURE,
            module.STATE_START_FAILURE,
            module.STATE_CLEANUP_FAILURE,
        }
        self.assertEqual(set(module.EXECUTION_STATES), expected)
        self.assertTrue(expected.isdisjoint(AGREEMENT))

    def test_error_states_use_capture_observe_error(self) -> None:
        module = _require()
        mapping = {
            module.STATE_TIMEOUT: STATE_CANDIDATE_ERROR,
            module.STATE_OOM: STATE_CANDIDATE_ERROR,
            module.STATE_OUTPUT_OVERFLOW: STATE_CANDIDATE_ERROR,
            module.STATE_PULL_FAILURE: STATE_CAPTURE_ERROR,
            module.STATE_CREATE_FAILURE: STATE_CAPTURE_ERROR,
            module.STATE_START_FAILURE: STATE_CAPTURE_ERROR,
            module.STATE_CLEANUP_FAILURE: STATE_CAPTURE_ERROR,
        }
        for state, capture_state in mapping.items():
            observation = module.observation_for(
                state,
                case_id="case-001",
                input_sha256="sha256:" + "ab" * 32,
                message=state,
            )
            self.assertEqual(observation["state"], capture_state)
            self.assertEqual(
                STATE_TO_STATUS[observation["state"]] in {"execution_error", "harness_error"},
                True,
            )
            self.assertNotIn(observation["state"], AGREEMENT)

    def test_completed_is_not_a_score(self) -> None:
        module = _require()
        with self.assertRaises(ValueError):
            module.observation_for(
                module.STATE_COMPLETED,
                case_id="case-001",
                input_sha256="sha256:" + "ab" * 32,
                message="ok",
            )


class NonClaimsAndReuse(unittest.TestCase):
    def test_non_claims_are_exact_issue_203_text(self) -> None:
        module = _require()
        self.assertEqual(module.OCI_EXECUTOR_NON_CLAIMS, (ISSUE_203_NON_CLAIMS,))


class DeclaredVolumes(unittest.TestCase):
    def test_declared_volumes_are_rejected(self) -> None:
        module = _require()
        with self.assertRaises(module.VolumeDeclarationError):
            module.reject_declared_volumes(
                {"Config": {"Volumes": {"/data": {}}}}
            )
        module.reject_declared_volumes({"Config": {"Volumes": None}})
        module.reject_declared_volumes({"Config": {}})


class LifecycleClassification(unittest.TestCase):
    def test_pull_create_start_cleanup_are_named_states(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(
                Path(raw),
                "does-not-exist.invalid/x@sha256:" + "ab" * 32,
            )
            bundle = _bundle(Path(raw))
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=bundle,
                registry_path=registry,
                timeout_seconds=2,
            )
        self.assertEqual(result.state, module.STATE_PULL_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_cleanup_failure_wins_over_completed(self) -> None:
        module = _require()
        inspect_doc = {
            "Id": "sha256:" + "cd" * 32,
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
            "State": {"OOMKilled": False, "ExitCode": 0, "Status": "exited"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(
                    0, json.dumps([inspect_doc]).encode(), b""
                )
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-1\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, b"", b"")
            if kind == "rm":
                raise module.DockerCommandError("rm refused")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=registry,
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_CLEANUP_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_start_failure_is_named_state(self) -> None:
        module = _require()
        inspect_doc = {
            "Id": "sha256:" + "cd" * 32,
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
            "State": {"OOMKilled": False, "ExitCode": 0, "Status": "created"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-start\n", b"")
            if kind == "start":
                raise module.DockerCommandError("cannot start container")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_START_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_create_failure_is_named_state(self) -> None:
        module = _require()

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(
                    0,
                    json.dumps([{"Config": {"Volumes": {"/data": {}}}}]).encode(),
                    b"",
                )
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_CREATE_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_create_oserror_is_named_create_failure(self) -> None:
        module = _require()
        inspect_doc = {
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                raise OSError("create refused")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_CREATE_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_cleanup_oserror_is_named_cleanup_failure(self) -> None:
        module = _require()
        inspect_doc = {
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
            "State": {"OOMKilled": False, "ExitCode": 0, "Status": "exited"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-os\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, b"", b"")
            if kind == "rm":
                raise OSError("rm refused")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_CLEANUP_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_lifecycle_errors_are_one_shared_tuple(self) -> None:
        module = _require()
        self.assertEqual(
            module.DOCKER_LIFECYCLE_ERRORS,
            (module.DockerCommandError, ProcessLimitError, OSError),
        )
        execute_src = inspect.getsource(module.execute_candidate)
        self.assertIn("DOCKER_LIFECYCLE_ERRORS", execute_src)
        self.assertNotIn("except (DockerCommandError", execute_src)


def _docker_info() -> subprocess.CompletedProcess[bytes]:
    return subprocess.run(
        ["docker", "info"],
        capture_output=True,
        timeout=20,
        check=False,
    )


def _compile_inert_linux_elf(dest: Path) -> None:
    """Host-compile a linux/amd64 static ELF. FROM scratch copies only this file."""
    errors: list[str] = []
    if sys.platform.startswith("linux"):
        compiler = shutil.which("gcc") or shutil.which("cc")
        if compiler is not None:
            compiled = subprocess.run(
                [compiler, "-static", "-Os", "-o", str(dest), str(FIXTURE_C)],
                capture_output=True,
            )
            if compiled.returncode == 0:
                return
            errors.append(compiled.stderr.decode("utf-8", "replace"))
    zig = shutil.which("zig")
    if zig is not None:
        compiled = subprocess.run(
            [
                zig,
                "cc",
                "-target",
                "x86_64-linux-musl",
                "-static",
                "-Os",
                "-o",
                str(dest),
                str(FIXTURE_C),
            ],
            capture_output=True,
        )
        if compiled.returncode == 0:
            return
        errors.append(compiled.stderr.decode("utf-8", "replace"))
    raise AssertionError(
        "could not host-compile the inert linux/amd64 ELF; the fixture "
        "image is FROM scratch and must not pull a compiler or candidate "
        "image: %s" % (" | ".join(errors) or "no gcc/zig")
    )


def _build_inert_image() -> tuple[str, str]:
    if not FIXTURE_C.is_file():
        raise AssertionError(f"missing fixture {FIXTURE_C}")
    with tempfile.TemporaryDirectory() as raw:
        context = Path(raw)
        _compile_inert_linux_elf(context / "oci-candidate")
        (context / "Dockerfile").write_text(
            "FROM scratch\nCOPY oci-candidate /oci-candidate\n",
            encoding="utf-8",
        )
        subprocess.run(
            [
                "docker",
                "build",
                "--platform",
                "linux/amd64",
                "-t",
                "assay-oci-inert:local",
                str(context),
            ],
            check=True,
            capture_output=True,
        )
    inspected = subprocess.run(
        ["docker", "inspect", "--format", "{{.Id}}", "assay-oci-inert:local"],
        check=True,
        capture_output=True,
        text=True,
    )
    image_id = inspected.stdout.strip()
    if not image_id.startswith("sha256:") or len(image_id) != 71:
        raise AssertionError(f"unexpected image id {image_id!r}")
    digest = image_id.split(":", 1)[1]
    registry_ref = f"assay-oci-inert@sha256:{digest}"
    implementations.validate_image_reference(registry_ref)
    return registry_ref, image_id


class LiveDockerInspect(unittest.TestCase):
    image_ref: str | None = None
    local_ref: str | None = None

    @classmethod
    def setUpClass(cls) -> None:
        info = _docker_info()
        if info.returncode != 0:
            raise AssertionError(
                "docker is required for live inspect; unavailable infrastructure "
                f"is not a pass: {info.stderr.decode('utf-8', 'replace')}"
            )
        cls.image_ref, cls.local_ref = _build_inert_image()

    def _local_runner(self) -> Any:
        module = _require()
        assert self.image_ref is not None
        assert self.local_ref is not None
        return local_image_docker_runner(
            registry_ref=self.image_ref,
            local_ref=self.local_ref,
        )

    def test_live_inspect_pins_network_and_user_independently_of_argv_strings(self) -> None:
        module = _require()
        assert self.image_ref is not None
        with tempfile.TemporaryDirectory() as raw:
            bundle = _bundle(Path(raw))
            staging = Path(raw) / "stage"
            argv = module.build_container_create_argv(
                image=self.image_ref,
                bundle_path=bundle,
                container_name=f"assay-oci-live-{os.getpid()}",
                staging_dir=staging,
                command=("/oci-candidate", "ok"),
            )
            assert_argv_network_none(argv)
            assert_argv_runtime_user(argv)
            self.assertIn(self.image_ref, argv)
            assert self.local_ref is not None
            live_argv = rewrite_fixture_image_argv(
                argv, registry_ref=self.image_ref, local_ref=self.local_ref
            )
            self.assertNotIn(self.image_ref, live_argv)
            created = subprocess.run(live_argv, capture_output=True, text=True, check=False)
            self.assertEqual(created.returncode, 0, created.stderr)
            container = created.stdout.strip()
            try:
                inspected = subprocess.run(
                    ["docker", "inspect", container],
                    capture_output=True,
                    text=True,
                    check=True,
                )
                doc = json.loads(inspected.stdout)[0]
                assert_inspect_network_none(doc)
                assert_inspect_runtime_user(doc)
                host = doc["HostConfig"]
                self.assertTrue(host.get("ReadonlyRootfs"))
                self.assertIn("ALL", host.get("CapDrop") or [])
                self.assertEqual(host.get("IpcMode"), "none")
                self.assertEqual(host.get("PidsLimit"), int(module.PIDS_LIMIT))
                self.assertEqual((host.get("RestartPolicy") or {}).get("Name"), "no")
                mounts = doc.get("Mounts") or []
                self.assertEqual(len(mounts), 1)
                self.assertEqual(
                    mounts[0]["Source"],
                    str((staging / "opaque.bundle").resolve()),
                )
                self.assertNotEqual(mounts[0]["Source"], str(bundle.resolve()))
                self.assertEqual(mounts[0]["Destination"], "/input/bundle.tar.gz")
                self.assertTrue(mounts[0]["RW"] is False)
                for forbidden in ("/var/run/docker.sock", str(REPO), str(Path.home())):
                    self.assertFalse(
                        any(forbidden in (mount.get("Source") or "") for mount in mounts)
                    )
            finally:
                subprocess.run(
                    ["docker", "rm", "--force", "--volumes", container],
                    capture_output=True,
                    check=False,
                )

    def test_live_volume_image_is_rejected_before_create(self) -> None:
        module = _require()
        assert self.image_ref is not None
        volume_tag = "assay-oci-inert-volume:local"
        subprocess.run(
            [
                "docker",
                "build",
                "--platform",
                "linux/amd64",
                "-t",
                volume_tag,
                "-",
            ],
            input=b"FROM assay-oci-inert:local\nVOLUME /data\n",
            check=True,
            capture_output=True,
        )
        image_id = subprocess.run(
            ["docker", "inspect", "--format", "{{.Id}}", volume_tag],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        inspect_raw = subprocess.run(
            ["docker", "image", "inspect", image_id],
            check=True,
            capture_output=True,
            text=True,
        )
        with self.assertRaises(module.VolumeDeclarationError):
            module.reject_declared_volumes(json.loads(inspect_raw.stdout)[0])

    def test_live_timeout_is_named_timeout_not_agreement(self) -> None:
        module = _require()
        assert self.image_ref is not None
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), self.image_ref)
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=registry,
                timeout_seconds=1,
                command=("/oci-candidate", "sleep"),
                docker_runner=self._local_runner(),
            )
        self.assertEqual(result.state, module.STATE_TIMEOUT)
        self.assertNotIn(result.state, AGREEMENT)

    def test_live_overflow_is_named_overflow_not_agreement(self) -> None:
        module = _require()
        assert self.image_ref is not None
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), self.image_ref)
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=registry,
                timeout_seconds=5,
                command=("/oci-candidate", "flood"),
                docker_runner=self._local_runner(),
            )
        self.assertEqual(result.state, module.STATE_OUTPUT_OVERFLOW)
        self.assertNotIn(result.state, AGREEMENT)

    def test_live_completed_ok_is_not_agreement(self) -> None:
        module = _require()
        assert self.image_ref is not None
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), self.image_ref)
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=registry,
                timeout_seconds=10,
                command=("/oci-candidate", "ok"),
                docker_runner=self._local_runner(),
            )
        self.assertEqual(result.state, module.STATE_COMPLETED)
        self.assertNotIn(result.state, AGREEMENT)
        self.assertEqual(result.exit_code, 0)

    def test_live_oom_is_named_oom_not_agreement(self) -> None:
        module = _require()
        assert self.image_ref is not None
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), self.image_ref)
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=registry,
                timeout_seconds=60,
                command=("/oci-candidate", "oom"),
                docker_runner=self._local_runner(),
            )
        self.assertEqual(result.state, module.STATE_OOM)
        self.assertNotIn(result.state, AGREEMENT)

    def test_live_create_survives_retag_of_local_tag(self) -> None:
        module = _require()
        assert self.image_ref is not None
        assert self.local_ref is not None
        self.assertTrue(self.local_ref.startswith("sha256:"))
        subprocess.run(
            [
                "docker",
                "build",
                "--platform",
                "linux/amd64",
                "-t",
                "assay-oci-inert-other:local",
                "-",
            ],
            input=b"FROM assay-oci-inert:local\nLABEL assay=other\n",
            check=True,
            capture_output=True,
        )
        other_id = subprocess.run(
            ["docker", "inspect", "--format", "{{.Id}}", "assay-oci-inert-other:local"],
            check=True,
            capture_output=True,
            text=True,
        ).stdout.strip()
        subprocess.run(["docker", "tag", other_id, "assay-oci-inert:local"], check=True)
        try:
            with tempfile.TemporaryDirectory() as raw:
                registry = _write_registry(Path(raw), self.image_ref)
                result = module.execute_candidate(
                    implementation_id="inert-fixture",
                    bundle_path=_bundle(Path(raw)),
                    registry_path=registry,
                    timeout_seconds=10,
                    command=("/oci-candidate", "ok"),
                    docker_runner=self._local_runner(),
                )
            tagged = subprocess.run(
                ["docker", "inspect", "--format", "{{.Id}}", "assay-oci-inert:local"],
                check=True,
                capture_output=True,
                text=True,
            ).stdout.strip()
            self.assertEqual(tagged, other_id)
            self.assertEqual(result.state, module.STATE_COMPLETED)
            self.assertNotEqual(self.local_ref, other_id)
        finally:
            subprocess.run(["docker", "tag", self.local_ref, "assay-oci-inert:local"], check=False)

    def test_fresh_docker_config_is_anonymous(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            env, config_dir = module.fresh_docker_env(Path(raw))
        self.assertEqual(env.get("DOCKER_CONFIG"), str(config_dir))
        self.assertFalse((config_dir / "config.json").exists())
        self.assertNotIn("REGISTRY_AUTH_FILE", env)


class BoundMutations(unittest.TestCase):
    def test_each_bound_has_a_biting_mutation(self) -> None:
        argv = _argv()
        cases = (
            ("--network", "host", assert_argv_network_none),
            ("--user", "0:0", assert_argv_runtime_user),
        )
        for flag, value, checker in cases:
            with self.subTest(flag=flag, value=value):
                with self.assertRaises(AssertionError):
                    checker(_replace_value(argv, flag, value))
        for flag in (
            "--read-only",
            "--cap-drop",
            "--security-opt",
            "--cpus",
            "--memory",
            "--memory-swap",
            "--pids-limit",
            "--ipc",
            "--tmpfs",
            "--restart",
            "--mount",
            "--platform",
            "--log-opt",
        ):
            with self.subTest(drop=flag):
                mutated = _drop_pair(argv, flag)
                self.assertNotEqual(mutated, argv)
                self.assertTrue(
                    flag not in mutated or flag == "--log-opt",
                    f"{flag} survived drop",
                )


SENTINEL_ENV = {
    "GITHUB_TOKEN": "sentinel-gh-token",
    "DOCKER_AUTH_CONFIG": "sentinel-docker-auth",
    "HTTP_PROXY": "http://sentinel-proxy.example",
    "HTTPS_PROXY": "http://sentinel-proxy.example",
    "AWS_SECRET_ACCESS_KEY": "sentinel-aws-key",
    "REGISTRY_AUTH_FILE": "/tmp/sentinel-registry-auth",
}
VALID_REPORT = b'{"bundle_integrity":"fail"}'


class AllowlistedDockerEnv(unittest.TestCase):
    def test_parent_secrets_do_not_reach_docker_cli_env(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            with mock.patch.dict(os.environ, SENTINEL_ENV, clear=False):
                env, config_dir = module.fresh_docker_env(Path(raw))
                wrapped = module.wrap_docker_command(["docker", "info"], env)
        for key, value in SENTINEL_ENV.items():
            self.assertNotIn(key, env)
            self.assertNotIn(value, " ".join(wrapped))
        self.assertEqual(env.get("DOCKER_CONFIG"), str(config_dir))
        self.assertFalse((config_dir / "config.json").exists())
        self.assertIn("-i", wrapped)

    def test_local_image_runner_uses_the_same_wrap(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            env, _ = module.fresh_docker_env(Path(raw))
        probe = module.wrap_docker_command(["docker", "image", "inspect", IMAGE_ID], env)
        self.assertIn("-i", probe)
        for key in SENTINEL_ENV:
            self.assertNotIn(key, " ".join(probe))
        source = Path(module.__file__).read_text(encoding="utf-8")
        self.assertIn("wrap_docker_command", source)
        self.assertNotIn("def rewrite_fixture_image_argv", source)
        self.assertNotIn("def local_image_docker_runner", source)

    def test_synthetic_repo_digest_is_rewritten_for_pull_inspect_and_create(self) -> None:
        """A local tag is not a RepoDigest. Hosted Docker treats name@sha256:<id>
        as a remote pull. The test runner must map that ref to an image id.
        """
        module = _require()
        pull = ["docker", "pull", "--platform", module.PLATFORM, DIGEST_IMAGE]
        inspect = ["docker", "image", "inspect", DIGEST_IMAGE]
        with tempfile.TemporaryDirectory() as raw:
            create = module.build_container_create_argv(
                image=DIGEST_IMAGE,
                bundle_path=_bundle(Path(raw)),
                container_name="assay-oci-map",
                staging_dir=Path(raw) / "stage",
            )
        self.assertEqual(create[-1], DIGEST_IMAGE)
        mapped_pull = rewrite_fixture_image_argv(
            pull, registry_ref=DIGEST_IMAGE, local_ref=IMAGE_ID
        )
        mapped_inspect = rewrite_fixture_image_argv(
            inspect, registry_ref=DIGEST_IMAGE, local_ref=IMAGE_ID
        )
        mapped_create = rewrite_fixture_image_argv(
            create, registry_ref=DIGEST_IMAGE, local_ref=IMAGE_ID
        )
        self.assertEqual(mapped_pull[-1], IMAGE_ID)
        self.assertEqual(mapped_inspect[-1], IMAGE_ID)
        self.assertEqual(mapped_create[-1], IMAGE_ID)
        self.assertNotIn(DIGEST_IMAGE, mapped_inspect)
        self.assertNotIn(DIGEST_IMAGE, mapped_create)
        implementations.validate_image_reference(DIGEST_IMAGE)
        with self.assertRaises(ValueError):
            rewrite_fixture_image_argv(
                pull, registry_ref=DIGEST_IMAGE, local_ref="assay-oci-inert:local"
            )

    def test_local_runner_does_not_send_synthetic_repo_digest_to_docker(self) -> None:
        module = _require()
        seen: list[list[str]] = []

        def fake_run_docker(argv: list[str], **kwargs: Any) -> Any:
            seen.append(list(argv))
            return module.BoundedDockerResult(0, b"[]", b"")

        with tempfile.TemporaryDirectory() as raw:
            env, _ = module.fresh_docker_env(Path(raw))
        runner = local_image_docker_runner(registry_ref=DIGEST_IMAGE, local_ref=IMAGE_ID)
        with mock.patch.object(module, "run_docker", side_effect=fake_run_docker):
            with mock.patch.object(module, "run_bounded") as bounded:
                bounded.return_value = mock.Mock(returncode=0, stdout=b"", stderr=b"")
                runner(
                    ["docker", "pull", "--platform", module.PLATFORM, DIGEST_IMAGE],
                    env=env,
                )
                runner(["docker", "image", "inspect", DIGEST_IMAGE], env=env)
                runner(["docker", "create", "--name", "x", DIGEST_IMAGE], env=env)
                probe = bounded.call_args[0][0]
        self.assertIn(IMAGE_ID, probe)
        self.assertNotIn(DIGEST_IMAGE, probe)
        self.assertEqual(len(seen), 2)
        for argv in seen:
            self.assertIn(IMAGE_ID, argv)
            self.assertNotIn(DIGEST_IMAGE, argv)


class StagedBundleMount(unittest.TestCase):
    def test_repo_and_home_inputs_are_staged_before_mount(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            caller = Path(raw) / "repo-or-home" / "secret.bundle"
            caller.parent.mkdir()
            caller.write_bytes(b"caller-bytes")
            staging = Path(raw) / "assay-oci-stage"
            staging.mkdir()
            argv = module.build_container_create_argv(
                image=DIGEST_IMAGE,
                bundle_path=caller,
                container_name="assay-oci-test",
                staging_dir=staging,
            )
            mount = _flag_value(argv, "--mount") or ""
            staged = (staging / "opaque.bundle").resolve()
            self.assertIn(f"src={staged}", mount)
            self.assertNotIn(str(caller.resolve()), mount)
            self.assertEqual(staged.read_bytes(), b"caller-bytes")

    def test_symlink_bundle_is_rejected_by_the_shared_reader(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            real = _bundle(Path(raw))
            link = Path(raw) / "link.bundle"
            link.symlink_to(real)
            with self.assertRaises(ValueError):
                module.stage_opaque_bundle(link, Path(raw) / "stage")

    def test_oversize_bundle_is_rejected_by_the_shared_reader(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            huge = Path(raw) / "huge.bundle"
            huge.write_bytes(b"x" * (module.MAX_BUNDLE_BYTES + 1))
            with self.assertRaises(ValueError):
                module.stage_opaque_bundle(huge, Path(raw) / "stage")

    def test_swap_after_read_does_not_change_staged_bytes(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            source = Path(raw) / "moving.bundle"
            source.write_bytes(b"first")
            staged = module.stage_opaque_bundle(source, Path(raw) / "stage")
            source.write_bytes(b"swapped")
            self.assertEqual(staged.read_bytes(), b"first")


class CandidateOutputHandoff(unittest.TestCase):
    def test_handoff_keeps_exact_bytes_out_of_trusted_metadata(self) -> None:
        module = _require()
        payload = b"\x00hostile-stdout\xff"
        result = module.OciExecution(
            module.STATE_COMPLETED,
            "inert-fixture",
            DIGEST_IMAGE,
            0,
            payload,
            b"err-bytes",
            "",
        )
        with tempfile.TemporaryDirectory() as raw:
            output = Path(raw) / "handoff"
            module.write_handoff(output, result)
            metadata = (output / module.EXECUTION_DOCUMENT_NAME).read_text(encoding="utf-8")
            stdout_file = (output / module.CANDIDATE_STDOUT_NAME).read_bytes()
            stderr_file = (output / module.CANDIDATE_STDERR_NAME).read_bytes()
            document = json.loads(metadata)
        self.assertEqual(stdout_file, payload)
        self.assertEqual(stderr_file, b"err-bytes")
        self.assertNotIn("hostile-stdout", metadata)
        self.assertNotIn(payload.decode("latin1"), metadata)
        self.assertNotIn("stdout", document)
        self.assertEqual(document["candidate_output"]["stdout"], module.CANDIDATE_STDOUT_NAME)
        self.assertTrue(document["candidate_output"]["stdout_sha256"].startswith("sha256:"))

    def test_handoff_rejects_symlink_nonempty_and_oversize(self) -> None:
        module = _require()
        result = module.OciExecution(
            module.STATE_COMPLETED,
            "inert-fixture",
            DIGEST_IMAGE,
            0,
            b"ok",
            b"",
            "",
        )
        with tempfile.TemporaryDirectory() as raw:
            nonempty = Path(raw) / "nonempty"
            nonempty.mkdir()
            (nonempty / "stale").write_text("x", encoding="utf-8")
            with self.assertRaises(ValueError):
                module.write_handoff(nonempty, result)
            as_file = Path(raw) / "not-a-dir"
            as_file.write_text("x", encoding="utf-8")
            with self.assertRaises(ValueError):
                module.write_handoff(as_file, result)
            target = Path(raw) / "real-dir"
            target.mkdir()
            link = Path(raw) / "link-dir"
            link.symlink_to(target)
            with self.assertRaises(ValueError):
                module.write_handoff(link, result)
            oversized = module.OciExecution(
                module.STATE_COMPLETED,
                "inert-fixture",
                DIGEST_IMAGE,
                0,
                b"x" * (module.STDOUT_LIMIT + 1),
                b"",
                "",
            )
            with self.assertRaises(ValueError):
                module.write_handoff(Path(raw) / "fresh", oversized)

    def test_write_execution_is_not_a_silent_parent_rewrite(self) -> None:
        module = _require()
        self.assertFalse(hasattr(module, "write_execution"))

    def _completed(self, stdout: bytes = b"ok") -> Any:
        module = _require()
        return module.OciExecution(
            module.STATE_COMPLETED,
            "inert-fixture",
            DIGEST_IMAGE,
            0,
            stdout,
            b"",
            "",
        )

    def test_preexisting_empty_dir_is_rejected(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            empty = Path(raw) / "empty"
            empty.mkdir()
            with self.assertRaises(ValueError):
                module.write_handoff(empty, self._completed())
            self.assertTrue(empty.is_dir())
            self.assertEqual(list(empty.iterdir()), [])

    def test_failed_rename_leaves_destination_absent_and_cleans_temp(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            dest = Path(raw) / "handoff"
            with mock.patch.object(os, "rename", side_effect=OSError("simulated rename failure")):
                with self.assertRaises(OSError):
                    module.write_handoff(dest, self._completed())
            self.assertFalse(dest.exists())
            leftovers = [
                path
                for path in Path(raw).iterdir()
                if path.name.startswith(module.HANDOFF_TEMP_PREFIX)
            ]
            self.assertEqual(leftovers, [])

    def _main_handoff(self, dest: Path, bundle: Path, *, rename_error: bool = False) -> tuple[int, str]:
        module = _require()
        stderr = mock.Mock()
        argv = [
            "--implementation-id",
            "inert-fixture",
            "--bundle",
            str(bundle),
            "--output",
            str(dest),
        ]
        with mock.patch.object(module, "execute_candidate", return_value=self._completed()):
            with mock.patch.object(sys, "stderr", stderr):
                if rename_error:
                    with mock.patch.object(
                        os, "rename", side_effect=OSError("simulated rename failure")
                    ):
                        code = module.main(argv)
                else:
                    code = module.main(argv)
        written = "".join(
            str(call.args[0]) for call in stderr.write.call_args_list if call.args
        )
        return code, written

    def test_main_handoff_existing_dest_exits_2_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            dest = Path(raw) / "handoff"
            dest.mkdir()
            (dest / "stale").write_bytes(b"keep-me")
            code, err = self._main_handoff(dest, _bundle(Path(raw)))
            self.assertEqual((dest / "stale").read_bytes(), b"keep-me")
        self.assertEqual(code, 2)
        self.assertTrue(err.strip())
        self.assertNotIn("Traceback", err)

    def test_main_handoff_symlink_dest_exits_2_without_traceback(self) -> None:
        with tempfile.TemporaryDirectory() as raw:
            target = Path(raw) / "real"
            target.mkdir()
            dest = Path(raw) / "handoff"
            dest.symlink_to(target)
            code, err = self._main_handoff(dest, _bundle(Path(raw)))
            self.assertTrue(dest.is_symlink())
            self.assertEqual(list(target.iterdir()), [])
        self.assertEqual(code, 2)
        self.assertTrue(err.strip())
        self.assertNotIn("Traceback", err)

    def test_main_handoff_rename_failure_exits_2_and_leaves_dest_absent(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            dest = Path(raw) / "handoff"
            code, err = self._main_handoff(dest, _bundle(Path(raw)), rename_error=True)
            self.assertFalse(dest.exists())
            leftovers = [
                path
                for path in Path(raw).iterdir()
                if path.name.startswith(module.HANDOFF_TEMP_PREFIX)
            ]
            self.assertEqual(leftovers, [])
        self.assertEqual(code, 2)
        self.assertTrue(err.strip())
        self.assertNotIn("Traceback", err)


class CaptureAdapterEquivalence(unittest.TestCase):
    """#199 capture_observations via the OCI adapter, no second loop."""

    def _pack(self, directory: Path) -> dict[str, Any]:
        cases = []
        for index in range(1, 15):
            case_id = f"case-{index:03d}"
            data = f"bundle-{index}".encode()
            path = directory / f"{case_id}.bundle.tar.gz"
            path.write_bytes(data)
            cases.append(
                {
                    "id": case_id,
                    "sha256": "sha256:" + hashlib.sha256(data).hexdigest(),
                    "_local_path": str(path),
                }
            )
        return {
            "declared_source_commit": "1" * 40,
            "source_corpus_digest": "sha256:" + "ab" * 32,
            "rendered_set_digest": "sha256:" + "cd" * 32,
            "cases": cases,
        }

    def _direct_candidate(self, directory: Path) -> list[str]:
        script = directory / "direct-candidate.py"
        script.write_text(
            "import sys\n"
            "sys.stdout.buffer.write(%r)\n" % (VALID_REPORT,),
            encoding="utf-8",
        )
        return [sys.executable, str(script)]

    def _completed_runner(self, stdout: bytes = VALID_REPORT):
        module = _require()
        inspect_doc = {
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
            "State": {"OOMKilled": False, "ExitCode": 0, "Status": "exited"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-eq\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, stdout, b"")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        return runner

    def test_adapter_matches_direct_capture_observations(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            pack = self._pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            direct = capture_candidate.capture_observations(
                pack, self._direct_candidate(Path(raw)), 5
            )
            command = module.oci_entrypoint_command(implementation_id="inert-fixture")
            runner = self._completed_runner()
            via_oci = capture_candidate.capture_observations(
                pack,
                command,
                5,
                candidate_runner=functools.partial(
                    module.run_oci_candidate,
                    docker_runner=runner,
                    registry_path=registry,
                ),
            )
            via_wrapper = module.capture_oci_observations(
                pack,
                command,
                5,
                registry_path=registry,
                docker_runner=runner,
            )
        self.assertEqual(via_oci, via_wrapper)
        self.assertEqual(via_oci, direct)
        identity = {
            "id": None,
            "image": None,
            "name": "eq",
            "version": None,
            "source": "https://example.example/eq",
            "commit": "1" * 40,
            "reproduction_mode": "other_disclosed",
        }
        first = capture_candidate.build_capture(pack, "sha256:" + "11" * 32, via_oci, identity)
        second = capture_candidate.build_capture(pack, "sha256:" + "11" * 32, direct, identity)
        capture_candidate.validate_capture(first)
        capture_candidate.validate_capture(second)
        self.assertEqual(first, second)

    def test_dropping_stdout_breaks_equivalence(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            pack = self._pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            direct = capture_candidate.capture_observations(
                pack, self._direct_candidate(Path(raw)), 5
            )
            dropped = capture_candidate.capture_observations(
                pack,
                module.oci_entrypoint_command(implementation_id="inert-fixture"),
                5,
                candidate_runner=functools.partial(
                    module.run_oci_candidate,
                    docker_runner=self._completed_runner(stdout=b""),
                    registry_path=registry,
                ),
            )
        self.assertNotEqual(dropped, direct)
        self.assertTrue(all(item["state"] == STATE_CANDIDATE_ERROR for item in dropped))

    def test_stdout_is_parsed_once_by_the_existing_parser(self) -> None:
        module = _require()
        calls: list[bytes] = []
        real = capture_candidate.parse_candidate_report

        def spy(stdout: bytes) -> dict[str, Any]:
            calls.append(stdout)
            return real(stdout)

        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            with mock.patch.object(capture_candidate, "parse_candidate_report", side_effect=spy):
                module.run_oci_candidate(
                    module.oci_entrypoint_command(implementation_id="inert-fixture"),
                    _bundle(Path(raw)),
                    5,
                    docker_runner=self._completed_runner(),
                    registry_path=registry,
                )
        self.assertEqual(calls, [VALID_REPORT])

    def test_logging_or_metadata_stdout_is_refused(self) -> None:
        module = _require()
        marker = b"MUST-NOT-LOG-STDOUT"
        runner = self._completed_runner(stdout=marker)
        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            with mock.patch.object(sys, "stdout", mock.Mock()) as fake_out:
                result = module.execute_candidate(
                    implementation_id="inert-fixture",
                    bundle_path=_bundle(Path(raw)),
                    registry_path=registry,
                    timeout_seconds=5,
                    docker_runner=runner,
                )
                module.write_handoff(Path(raw) / "out", result)
            written = b"".join(
                call.args[0] if isinstance(call.args[0], bytes) else call.args[0].encode()
                for call in fake_out.write.call_args_list
                if call.args
            )
            metadata = (Path(raw) / "out" / module.EXECUTION_DOCUMENT_NAME).read_text(
                encoding="utf-8"
            )
        self.assertEqual(result.stdout, marker)
        self.assertNotIn(marker, written)
        self.assertNotIn("MUST-NOT-LOG-STDOUT", metadata)

    def test_public_api_does_not_monkeypatch_run_candidate(self) -> None:
        tree = ast.parse(Path(__file__).read_text(encoding="utf-8"))
        patched = [
            node
            for node in ast.walk(tree)
            if isinstance(node, ast.Assign)
            and any(
                isinstance(target, ast.Attribute) and target.attr == "run_candidate"
                for target in node.targets
            )
        ]
        self.assertEqual(patched, [])
        params = inspect.signature(capture_candidate.capture_observations).parameters
        self.assertIn("candidate_runner", params)
        self.assertEqual(params["candidate_runner"].kind, inspect.Parameter.KEYWORD_ONLY)
        self.assertIs(params["candidate_runner"].default, capture_candidate.run_candidate)
        wrapper = inspect.getsource(_require().capture_oci_observations)
        self.assertIn("candidate_runner", wrapper)
        self.assertIn("run_oci_candidate", wrapper)
        self.assertNotIn("for ", wrapper)

    def test_parse_oci_command_rejects_unknown_image_flag(self) -> None:
        module = _require()
        with self.assertRaises(SystemExit):
            module.parse_oci_command(
                [
                    "--implementation-id",
                    "inert-fixture",
                    "--implementation-image",
                    DIGEST_IMAGE,
                ]
            )


def _write_pack(directory: Path) -> Path:
    dest = directory / "pack.tar.gz"
    commit = subprocess.check_output(
        ["git", "-C", str(REPO), "rev-parse", "HEAD"],
        text=True,
    ).strip()
    built = subprocess.run(
        [
            sys.executable,
            str(SCRIPTS / "build_clean_room_pack.py"),
            "--repo-root",
            str(REPO),
            "--source-commit",
            commit,
            "--output",
            str(dest),
        ],
        capture_output=True,
        text=True,
    )
    if built.returncode != 0:
        raise AssertionError(built.stderr)
    return dest


class PackCaptureCli(unittest.TestCase):
    """Pack + registry id → validated candidate_capture.v0. No second loop."""

    def _fake_runner(self, *_args: Any, **_kwargs: Any) -> dict[str, Any]:
        return {
            "exit_code": 0,
            "report": {"bundle_integrity": "fail"},
            "stderr_present": False,
        }

    def _completed_docker(self, pulled: list[str]):
        module = _require()
        inspect_doc = {
            "Config": {"Volumes": None, "User": "65532:65532"},
            "HostConfig": {"NetworkMode": "none"},
            "State": {"OOMKilled": False, "ExitCode": 0, "Status": "exited"},
        }

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                pulled.append(argv[-1])
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-reg\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, VALID_REPORT, b"")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        return runner

    def test_chosen_registry_feeds_executor_and_identity(self) -> None:
        module = _require()
        other_image = (
            "ghcr.io/example/other@sha256:"
            "abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789"
        )
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            chosen_doc = _registry_doc(DIGEST_IMAGE)
            chosen_doc["implementations"][0]["source"] = "https://example.example/chosen"
            chosen = Path(raw) / "chosen.json"
            chosen.write_text(json.dumps(chosen_doc), encoding="utf-8")
            other_doc = _registry_doc(other_image)
            other_doc["implementations"][0]["source"] = "https://example.example/other"
            (Path(raw) / "other.json").write_text(json.dumps(other_doc), encoding="utf-8")
            output = Path(raw) / "capture.json"
            pulled: list[str] = []
            module.write_validated_capture(
                pack,
                "inert-fixture",
                output,
                registry_path=chosen,
                docker_runner=self._completed_docker(pulled),
                timeout_seconds=5,
            )
            document = json.loads(output.read_text(encoding="utf-8"))
        validate_capture(document)
        self.assertEqual(set(pulled), {DIGEST_IMAGE})
        self.assertNotIn(other_image, pulled)
        self.assertEqual(document["implementation"]["image"], DIGEST_IMAGE)
        self.assertEqual(document["implementation"]["source"], "https://example.example/chosen")
        self.assertNotEqual(document["implementation"]["source"], "https://example.example/other")

    def test_cli_writes_validated_capture_from_pack_and_id(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            output = Path(raw) / "capture.json"
            printed = mock.Mock()
            parsed = module.parse_args(
                [
                    "--pack",
                    str(pack),
                    "--implementation-id",
                    "inert-fixture",
                    "--output",
                    str(output),
                ]
            )
            self.assertFalse(hasattr(parsed, "registry"))
            with (
                mock.patch.object(module, "run_oci_candidate", side_effect=self._fake_runner),
                mock.patch.object(sys, "stdout", printed),
            ):
                module.write_validated_capture(
                    pack,
                    "inert-fixture",
                    output,
                    registry_path=registry,
                    timeout_seconds=30,
                )
            document = json.loads(output.read_text(encoding="utf-8"))
        validate_capture(document)
        self.assertEqual(document["schema"], CAPTURE_SCHEMA)
        self.assertEqual(document["implementation"]["id"], "inert-fixture")
        self.assertEqual(document["implementation"]["image"], DIGEST_IMAGE)
        written = "".join(
            str(call.args[0]) for call in printed.write.call_args_list if call.args
        )
        self.assertNotIn("bundle_integrity", written)
        self.assertNotIn("hostile", written)

    def test_bypassing_validate_capture_does_not_write(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            output = Path(raw) / "capture.json"
            with (
                mock.patch.object(module, "run_oci_candidate", side_effect=self._fake_runner),
                mock.patch.object(
                    capture_candidate,
                    "build_capture",
                    return_value={"schema": "not-a-capture"},
                ),
            ):
                with self.assertRaises(ValueError):
                    module.write_validated_capture(
                        pack,
                        "inert-fixture",
                        output,
                        registry_path=registry,
                        timeout_seconds=30,
                    )
            self.assertFalse(output.exists())

    def test_stale_capture_stays_byte_identical_on_validation_failure(self) -> None:
        module = _require()
        stale = b'{"stale":true}\n'
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            output = Path(raw) / "capture.json"
            output.write_bytes(stale)
            with (
                mock.patch.object(module, "run_oci_candidate", side_effect=self._fake_runner),
                mock.patch.object(
                    capture_candidate,
                    "build_capture",
                    return_value={"schema": "not-a-capture"},
                ),
            ):
                with self.assertRaises(ValueError):
                    module.write_validated_capture(
                        pack,
                        "inert-fixture",
                        output,
                        registry_path=registry,
                        timeout_seconds=30,
                    )
            self.assertEqual(output.read_bytes(), stale)

    def test_stale_capture_stays_byte_identical_on_write_failure(self) -> None:
        module = _require()
        stale = b'{"stale":true}\n'
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            output = Path(raw) / "capture.json"
            output.write_bytes(stale)
            with (
                mock.patch.object(module, "run_oci_candidate", side_effect=self._fake_runner),
                mock.patch.object(os, "replace", side_effect=OSError("write interrupted")),
            ):
                with self.assertRaises(OSError):
                    module.write_validated_capture(
                        pack,
                        "inert-fixture",
                        output,
                        registry_path=registry,
                        timeout_seconds=30,
                    )
            self.assertEqual(output.read_bytes(), stale)
            leftovers = [
                path
                for path in Path(raw).iterdir()
                if path.name.startswith(module.CAPTURE_TEMP_PREFIX)
            ]
            self.assertEqual(leftovers, [])

    def test_pack_cli_rejects_direct_image(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            result = subprocess.run(
                [
                    sys.executable,
                    str(Path(module.__file__)),
                    "--pack",
                    str(Path(raw) / "pack.tar.gz"),
                    "--implementation-id",
                    "inert-fixture",
                    "--implementation-image",
                    DIGEST_IMAGE,
                    "--output",
                    str(Path(raw) / "capture.json"),
                ],
                cwd=REPO,
                capture_output=True,
                text=True,
            )
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("unrecognized arguments", result.stderr)
        source = Path(module.__file__).read_text(encoding="utf-8")
        self.assertNotIn("--implementation-image", source)


class EvidenceBinding(unittest.TestCase):
    """Close registry/pack/digest/lifecycle binding. Mutations must bite."""

    def _completed_docker(self, pulled: list[str], *, swap_registry: Path | None = None):
        module = _require()
        inspect_doc = _inspect_doc()

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                if swap_registry is not None and not pulled:
                    swap_registry.write_text(
                        json.dumps(_registry_doc(SWAPPED_IMAGE)), encoding="utf-8"
                    )
                pulled.append(argv[-1])
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-bind\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, VALID_REPORT, b"")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        return runner

    def test_registry_swap_cannot_split_identity_from_execution(self) -> None:
        module = _require()
        pulled: list[str] = []
        with tempfile.TemporaryDirectory() as raw:
            pack = _write_pack(Path(raw))
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            output = Path(raw) / "capture.json"
            module.write_validated_capture(
                pack,
                "inert-fixture",
                output,
                registry_path=registry,
                docker_runner=self._completed_docker(pulled, swap_registry=registry),
                timeout_seconds=5,
            )
            document = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(len(pulled), 14)
        self.assertEqual(set(pulled), {DIGEST_IMAGE})
        self.assertNotIn(SWAPPED_IMAGE, pulled)
        self.assertEqual(document["implementation"]["image"], DIGEST_IMAGE)
        validate_capture(document)

    def test_pack_swap_cannot_split_parsed_bytes_from_digest(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            pack_path = _write_pack(Path(raw))
            original = pack_path.read_bytes()
            digest_a = "sha256:" + hashlib.sha256(original).hexdigest()
            reads = {"n": 0}
            real_read = capture_candidate.read_pack_bytes

            def read_then_swap(path: Path) -> bytes:
                data = real_read(path)
                reads["n"] += 1
                if reads["n"] == 1:
                    path.write_bytes(original + b"\x00")
                return data

            output = Path(raw) / "capture.json"
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            with mock.patch.object(
                capture_candidate, "read_pack_bytes", side_effect=read_then_swap
            ):
                module.write_validated_capture(
                    pack_path,
                    "inert-fixture",
                    output,
                    registry_path=registry,
                    docker_runner=self._completed_docker([]),
                    timeout_seconds=5,
                )
            document = json.loads(output.read_text(encoding="utf-8"))
        self.assertEqual(document["pack_sha256"], digest_a)
        self.assertEqual(reads["n"], 1)

    def test_run_docker_forwards_digest_ref_to_wrapped_argv(self) -> None:
        module = _require()
        seen: list[list[str]] = []

        def fake_bounded(argv: list[str], **_kwargs: Any) -> Any:
            seen.append(list(argv))
            return mock.Mock(returncode=0, stdout=b"", stderr=b"")

        with tempfile.TemporaryDirectory() as raw:
            env, _ = module.fresh_docker_env(Path(raw))
        with mock.patch.object(module, "run_bounded", side_effect=fake_bounded):
            module.run_docker(
                ["docker", "pull", "--platform", module.PLATFORM, DIGEST_IMAGE],
                env=env,
            )
        self.assertEqual(len(seen), 1)
        self.assertIn(DIGEST_IMAGE, seen[0])
        self.assertTrue(any("@sha256:" in item for item in seen[0]))

    def test_digest_strip_at_wrap_bites(self) -> None:
        module = _require()
        original = module.wrap_docker_command

        def strip_digest(argv: list[str], env: dict[str, str]) -> list[str]:
            wrapped = original(argv, env)
            return [item.split("@sha256:")[0] if "@sha256:" in item else item for item in wrapped]

        with tempfile.TemporaryDirectory() as raw:
            env, _ = module.fresh_docker_env(Path(raw))
        with mock.patch.object(module, "wrap_docker_command", side_effect=strip_digest):
            with mock.patch.object(
                module, "run_bounded", return_value=mock.Mock(returncode=0, stdout=b"", stderr=b"")
            ):
                with self.assertRaises(module.DockerCommandError):
                    module.run_docker(
                        ["docker", "pull", "--platform", module.PLATFORM, DIGEST_IMAGE],
                        env=env,
                    )

    def test_missing_docker_is_named_pull_failure(self) -> None:
        module = _require()
        with tempfile.TemporaryDirectory() as raw:
            with mock.patch.object(
                module,
                "resolve_docker_executable",
                side_effect=module.DockerCommandError("docker executable not found"),
            ):
                result = module.execute_candidate(
                    implementation_id="inert-fixture",
                    bundle_path=_bundle(Path(raw)),
                    registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                    timeout_seconds=2,
                )
        self.assertEqual(result.state, module.STATE_PULL_FAILURE)
        self.assertNotIn(result.state, AGREEMENT)

    def test_missing_docker_main_is_not_a_traceback(self) -> None:
        module = _require()
        stdout = mock.Mock()
        stderr = mock.Mock()
        with tempfile.TemporaryDirectory() as raw:
            bundle = _bundle(Path(raw))
            row = _registry_doc(DIGEST_IMAGE)["implementations"][0]
            with (
                mock.patch.object(
                    module,
                    "resolve_docker_executable",
                    side_effect=module.DockerCommandError("docker executable not found"),
                ),
                mock.patch.object(module, "implementation_from_registry", return_value=row),
                mock.patch.object(sys, "stdout", stdout),
                mock.patch.object(sys, "stderr", stderr),
            ):
                code = module.main(
                    [
                        "--implementation-id",
                        "inert-fixture",
                        "--bundle",
                        str(bundle),
                    ]
                )
        out = "".join(str(call.args[0]) for call in stdout.write.call_args_list if call.args)
        err = "".join(str(call.args[0]) for call in stderr.write.call_args_list if call.args)
        self.assertEqual(code, 0)
        self.assertEqual(out.strip(), module.STATE_PULL_FAILURE)
        self.assertNotIn("Traceback", out)
        self.assertNotIn("Traceback", err)

    def test_create_timeout_cleans_up_by_container_name(self) -> None:
        module = _require()
        removed: list[str] = []
        created_name = {"value": ""}

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([_inspect_doc()]).encode(), b"")
            if kind == "create":
                created_name["value"] = argv[argv.index("--name") + 1]
                raise ProcessLimitError("timed out after the daemon created the container")
            if kind == "rm":
                removed.append(argv[-1])
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_CREATE_FAILURE)
        self.assertEqual(removed, [created_name["value"]])
        self.assertTrue(created_name["value"].startswith("assay-oci-"))

    def test_cleanup_failure_does_not_overwrite_timeout(self) -> None:
        module = _require()

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(0, json.dumps([_inspect_doc()]).encode(), b"")
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-timeout\n", b"")
            if kind == "start":
                raise ProcessLimitError("timed out waiting for candidate")
            if kind == "rm":
                raise module.DockerCommandError("rm refused after timeout")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=_bundle(Path(raw)),
                registry_path=_write_registry(Path(raw), DIGEST_IMAGE),
                timeout_seconds=2,
                docker_runner=runner,
            )
        self.assertEqual(result.state, module.STATE_TIMEOUT)
        self.assertIn("cleanup", result.error)
        self.assertLessEqual(len(result.error), MAX_ERROR_CHARS)

    def test_handoff_error_uses_canonical_bound(self) -> None:
        module = _require()
        long_error = "e" * (MAX_ERROR_CHARS * 4)
        result = module.OciExecution(
            module.STATE_START_FAILURE,
            "inert-fixture",
            DIGEST_IMAGE,
            None,
            b"",
            b"",
            long_error,
        )
        with tempfile.TemporaryDirectory() as raw:
            module.write_handoff(Path(raw) / "out", result)
            document = json.loads(
                (Path(raw) / "out" / module.EXECUTION_DOCUMENT_NAME).read_text(encoding="utf-8")
            )
        self.assertEqual(document["error"], bound_error(long_error))
        self.assertLessEqual(len(document["error"]), MAX_ERROR_CHARS)

    def test_missing_exit_code_is_harness_error_not_zero(self) -> None:
        module = _require()

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(
                    0, json.dumps([_inspect_doc(exit_code=None)]).encode(), b""
                )
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-exit\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, VALID_REPORT, b"")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            bundle = _bundle(Path(raw))
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=bundle,
                registry_path=registry,
                timeout_seconds=2,
                docker_runner=runner,
            )
            with self.assertRaises(capture_candidate.HarnessError):
                module.run_oci_candidate(
                    module.oci_entrypoint_command(implementation_id="inert-fixture"),
                    bundle,
                    2,
                    docker_runner=runner,
                    registry_path=registry,
                )
        self.assertNotEqual(result.exit_code, 0)
        self.assertNotEqual(result.state, module.STATE_COMPLETED)
        self.assertIn(result.state, module.HARNESS_FAILURE_STATES)

    def test_bool_exit_code_is_harness_error_not_zero_or_one(self) -> None:
        module = _require()

        def runner(argv: list[str], **_kwargs: Any) -> Any:
            kind = _docker_kind(argv)
            if kind == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if kind in {"inspect", "image-inspect"}:
                return module.BoundedDockerResult(
                    0, json.dumps([_inspect_doc(exit_code=True)]).encode(), b""
                )
            if kind == "create":
                return module.BoundedDockerResult(0, b"cid-bool\n", b"")
            if kind == "start":
                return module.BoundedDockerResult(0, VALID_REPORT, b"")
            if kind == "rm":
                return module.BoundedDockerResult(0, b"", b"")
            raise AssertionError(argv)

        with tempfile.TemporaryDirectory() as raw:
            registry = _write_registry(Path(raw), DIGEST_IMAGE)
            bundle = _bundle(Path(raw))
            result = module.execute_candidate(
                implementation_id="inert-fixture",
                bundle_path=bundle,
                registry_path=registry,
                timeout_seconds=2,
                docker_runner=runner,
            )
            with self.assertRaises(capture_candidate.HarnessError):
                module.run_oci_candidate(
                    module.oci_entrypoint_command(implementation_id="inert-fixture"),
                    bundle,
                    2,
                    docker_runner=runner,
                    registry_path=registry,
                )
        self.assertNotIn(result.exit_code, (0, 1, True, False))
        self.assertNotEqual(result.state, module.STATE_COMPLETED)
        self.assertIn(result.state, module.HARNESS_FAILURE_STATES)


if __name__ == "__main__":
    unittest.main()
