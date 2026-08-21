#!/usr/bin/env python3
"""Canonical bounded OCI executor contract (assay-tunnel-experiments #203).

    python3 -W error::ResourceWarning \\
        conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py

Argv pins and live `docker inspect` assertions are independent. Mutating the
canonical builder must break both, not a workflow comment.
"""

from __future__ import annotations

import ast
import hashlib
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
CONFORMANCE_WORKFLOW = REPO / ".github/workflows/privileged-mcp-action-conformance.yml"

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
from capture_format import STATE_CANDIDATE_ERROR, STATE_CAPTURE_ERROR
from score_candidate import STATE_TO_STATUS

DIGEST_IMAGE = (
    "ghcr.io/example/checker@sha256:"
    "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"
)
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
            if argv[:2] == ["docker", "pull"] or argv[1] == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if argv[:2] == ["docker", "inspect"] or argv[1] == "inspect":
                return module.BoundedDockerResult(
                    0, json.dumps([inspect_doc]).encode(), b""
                )
            if argv[:2] == ["docker", "create"] or argv[1] == "create":
                return module.BoundedDockerResult(0, b"cid-1\n", b"")
            if argv[:2] == ["docker", "start"] or argv[1] == "start":
                return module.BoundedDockerResult(0, b"", b"")
            if argv[:2] == ["docker", "rm"] or argv[1] == "rm":
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
            if argv[1] == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if argv[1] == "inspect":
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if argv[1] == "create":
                return module.BoundedDockerResult(0, b"cid-start\n", b"")
            if argv[1] == "start":
                raise module.DockerCommandError("cannot start container")
            if argv[1] == "rm":
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
            if argv[1] == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if argv[1] == "inspect":
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


class ConformanceCiContract(unittest.TestCase):
    def test_existing_workflow_runs_this_file(self) -> None:
        text = CONFORMANCE_WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("test_activation_kit.py", text)
        self.assertIn("test_oci_candidate_executor.py", text)
        self.assertRegex(
            text,
            r"python3 -m unittest \\\n"
            r"\s+conformance/privileged-mcp-action-v0/tests/test_activation_kit.py \\\n"
            r"\s+conformance/privileged-mcp-action-v0/tests/test_oci_candidate_executor.py",
        )


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


def _build_inert_image() -> str:
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
    ref = f"assay-oci-inert@sha256:{digest}"
    subprocess.run(["docker", "tag", image_id, f"assay-oci-inert:{digest}"], check=True)
    return ref


class LiveDockerInspect(unittest.TestCase):
    image_ref: str | None = None

    @classmethod
    def setUpClass(cls) -> None:
        info = _docker_info()
        if info.returncode != 0:
            raise AssertionError(
                "docker is required for live inspect; unavailable infrastructure "
                f"is not a pass: {info.stderr.decode('utf-8', 'replace')}"
            )
        cls.image_ref = _build_inert_image()

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
            created = subprocess.run(argv, capture_output=True, text=True, check=False)
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
        digest = image_id.split(":", 1)[1]
        volume_ref = f"assay-oci-inert-volume@sha256:{digest}"
        subprocess.run(["docker", "tag", image_id, f"assay-oci-inert-volume:{digest}"], check=True)
        inspect_raw = subprocess.run(
            ["docker", "inspect", volume_ref],
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
                docker_runner=module.local_image_docker_runner(),
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
                docker_runner=module.local_image_docker_runner(),
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
                docker_runner=module.local_image_docker_runner(),
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
                docker_runner=module.local_image_docker_runner(),
            )
        self.assertEqual(result.state, module.STATE_OOM)
        self.assertNotIn(result.state, AGREEMENT)

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
        probe = module.wrap_docker_command(["docker", "image", "inspect", DIGEST_IMAGE], env)
        self.assertIn("-i", probe)
        for key in SENTINEL_ENV:
            self.assertNotIn(key, " ".join(probe))
        source = Path(module.__file__).read_text(encoding="utf-8")
        self.assertIn("wrap_docker_command", source)
        self.assertNotIn('run_bounded(\n                ["docker", "image", "inspect"', source)


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
            if argv[1] == "pull":
                return module.BoundedDockerResult(0, b"", b"")
            if argv[1] == "inspect":
                return module.BoundedDockerResult(0, json.dumps([inspect_doc]).encode(), b"")
            if argv[1] == "create":
                return module.BoundedDockerResult(0, b"cid-eq\n", b"")
            if argv[1] == "start":
                return module.BoundedDockerResult(0, stdout, b"")
            if argv[1] == "rm":
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
            command = module.oci_entrypoint_command(
                implementation_id="inert-fixture",
                registry_path=registry,
            )
            runner = self._completed_runner()

            def adapted(cmd: list[str], bundle: Path, timeout: int) -> dict[str, Any]:
                return module.run_oci_candidate(
                    cmd, bundle, timeout, docker_runner=runner
                )

            original = capture_candidate.run_candidate
            capture_candidate.run_candidate = adapted
            try:
                via_oci = capture_candidate.capture_observations(pack, command, 5)
            finally:
                capture_candidate.run_candidate = original
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
            runner = self._completed_runner(stdout=b"")
            original = capture_candidate.run_candidate
            capture_candidate.run_candidate = (
                lambda cmd, bundle, timeout: module.run_oci_candidate(
                    cmd, bundle, timeout, docker_runner=runner
                )
            )
            try:
                dropped = capture_candidate.capture_observations(
                    pack,
                    module.oci_entrypoint_command(
                        implementation_id="inert-fixture",
                        registry_path=registry,
                    ),
                    5,
                )
            finally:
                capture_candidate.run_candidate = original
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
                    module.oci_entrypoint_command(
                        implementation_id="inert-fixture",
                        registry_path=registry,
                    ),
                    _bundle(Path(raw)),
                    5,
                    docker_runner=self._completed_runner(),
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


if __name__ == "__main__":
    unittest.main()
