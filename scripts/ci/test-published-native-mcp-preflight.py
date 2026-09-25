#!/usr/bin/env python3
"""Behavioral argv tests for the published native MCP preflight.

These execute the preflight orchestration with a mocked runner. They do not
install from crates.io, attest a release archive, or prove hosted Windows.
"""

from __future__ import annotations

import hashlib
import importlib.util
import json
import os
from pathlib import Path, PureWindowsPath
import re
import sys
import tempfile
import unittest
from unittest.mock import patch


ROOT = Path(__file__).resolve().parents[2]
PREFLIGHT = ROOT / "scripts/ci/published-native-mcp-preflight.py"
WORKFLOW = ROOT / ".github/workflows/published-native-mcp-preflight.yml"
CI_WORKFLOW = ROOT / ".github/workflows/ci.yml"
CI_SCOPE_STEP = "Published native MCP preflight contract"
CI_SCOPE_COMMAND = "python3 scripts/ci/test-published-native-mcp-preflight.py"
CI_SCOPE_ACTIVE = ("set -euo pipefail", CI_SCOPE_COMMAND)
PINNED_ACTIONS = (
    "actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09",
    "./.github/actions/setup-rust",
    "actions/upload-artifact@043fb46d1a93c77aae656e7c1c64a875d1fc6a0a",
)
DENY_STDOUT = (
    '{"jsonrpc":"2.0","id":1,"result":{}}\n'
    '{"jsonrpc":"2.0","id":9,"error":{"code":-31999,'
    '"data":{"origin":"assay-proxy","reason":"no_declared_allowance"}}}\n'
)
ALLOW_STDOUT = (
    '{"jsonrpc":"2.0","id":1,"result":{}}\n'
    '{"jsonrpc":"2.0","id":9,"result":{"isError":false,"content":['
    '{"type":"text","text":"forwarded-ok (mock; no real GitHub call)"}]}}\n'
)
# GitHub-hosted Windows runners invoke this Git Bash. System32\bash.exe is the WSL launcher.
GIT_BASH = r"C:\Program Files\Git\bin\bash.exe"
GIT_BASH_USR = r"C:\Program Files\Git\usr\bin\bash.exe"
WSL_BASH = r"C:\Windows\System32\bash.exe"
SUPPLIED_BASH = r"D:\Tools\Git\bin\bash.exe"
REQUIRED_RETAINED = {
    "cli-opening/results/attestation-verify.log",
    "cli-provenance.json",
    "crate-provenance.json",
    "installed-mcp-identity.json",
    "release-tag-reader.command.json",
    "release-tag-reader.stderr",
    "release-tag-reader.stdout",
    "status.json",
}


def retained_files(results: Path) -> set[str]:
    return {str(path.relative_to(results)) for path in results.rglob("*") if path.is_file()}


def scripted_isfile(existing: set[str]):
    original = os.path.isfile
    wanted = {path.casefold() for path in existing}

    def isfile(path) -> bool:
        if str(path).casefold() in wanted:
            return True
        return original(path)

    return isfile


def argv_for(world: FakeWorld, filename: str) -> list[str]:
    for call in world.calls:
        argv = call.get("argv") or []
        if any(str(part).replace("\\", "/").endswith(filename) for part in argv):
            return argv
    raise AssertionError(f"no recorded argv ended with {filename}")


def load_preflight_source(path: Path, source: str):
    path.write_text(source, encoding="utf-8")
    spec = importlib.util.spec_from_file_location(path.stem.replace("-", "_"), path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def load_preflight():
    spec = importlib.util.spec_from_file_location("published_native_mcp_preflight", PREFLIGHT)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


class FakeWorld:
    def __init__(self, module, run_root: Path) -> None:
        self.module = module
        self.run_root = run_root
        self.pin = "v6.6.2"
        self.mcp_version = "assay-mcp-server 6.6.2"
        self.deny_status = 0
        self.allow_status = 0
        self.deny_stdout = DENY_STDOUT
        self.allow_stdout = ALLOW_STDOUT
        self.crate_checksum = "ab" * 32
        self.yanked = False
        self.calls: list[dict] = []
        self.place_binary_outside_root = False
        self.reader_status = 0
        self.reader_stdout = self.pin.encode()
        self.reader_stderr = b""

    def download(self, url: str, destination: Path, *, max_bytes: int) -> None:
        self.calls.append({"kind": "download", "url": url, "destination": str(destination)})
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(
            json.dumps(
                {
                    "version": {
                        "num": self.pin[1:],
                        "yanked": self.yanked,
                        "checksum": self.crate_checksum,
                    }
                }
            ),
            encoding="utf-8",
        )
        if max_bytes < destination.stat().st_size:
            raise self.module.PreflightError("crate metadata exceeds ceiling")

    def run(self, argv, **kwargs):
        record = {
            "argv": list(argv),
            "env": dict(kwargs.get("env") or {}),
            "input": kwargs.get("input"),
            "cwd": str(kwargs.get("cwd") or ""),
        }
        self.calls.append(record)
        joined = " ".join(str(part) for part in argv)
        if "read-assay-release-tag.sh" in joined:
            return self.module.CommandResult(
                self.reader_status, self.reader_stdout, self.reader_stderr, list(argv)
            )
        if "published-release-platform-opening.sh" in joined:
            opening = Path(argv[argv.index("--run-root") + 1]) / "results"
            opening.mkdir(parents=True, exist_ok=True)
            (opening / "attestation-verify.log").write_text("attested\n", encoding="utf-8")
            (opening / "download-url.txt").write_text("https://example.test/cli.zip\n", encoding="utf-8")
            return self.module.CommandResult(0, b"ok: opening\n", b"", list(argv))
        if argv and argv[0] == "cargo":
            root = Path(argv[argv.index("--root") + 1])
            binary_dir = (self.run_root / "outside-bin") if self.place_binary_outside_root else (root / "bin")
            binary_dir.mkdir(parents=True, exist_ok=True)
            (binary_dir / "assay-mcp-server").write_text("fake\n", encoding="utf-8")
            return self.module.CommandResult(0, b"", b"", list(argv))
        if argv and Path(str(argv[0])).name in {"assay-mcp-server", "assay-mcp-server.exe"}:
            if "--version" in argv:
                return self.module.CommandResult(0, f"{self.mcp_version}\n".encode(), b"", list(argv))
            policy = Path(argv[argv.index("--enforce-policy") + 1]).name
            if policy == "allow.yaml":
                return self.module.CommandResult(
                    self.allow_status, self.allow_stdout.encode(), b"", list(argv)
                )
            return self.module.CommandResult(
                self.deny_status, self.deny_stdout.encode(), b"", list(argv)
            )
        raise AssertionError(f"unexpected argv: {argv}")

    def command_argv(self, needle: str) -> list[str]:
        for call in self.calls:
            argv = call.get("argv")
            if argv and needle in " ".join(str(part) for part in argv):
                return argv
        raise AssertionError(f"no recorded argv contained {needle}")


class PublishedNativeMcpPreflightTests(unittest.TestCase):
    def setUp(self) -> None:
        self.module = load_preflight()
        self.temporary = Path(self.enterContext(tempfile.TemporaryDirectory(prefix="native-mcp-preflight-")))
        self.run_root = self.temporary / "run"
        self.world = FakeWorld(self.module, self.run_root)

    def run_preflight(self, **overrides):
        kwargs = {
            "run_root": self.run_root,
            "repo_root": ROOT,
            "runner": self.world.run,
            "download": self.world.download,
            "environ": {"PATH": "/bin", "GH_TOKEN": "must-not-reach-cargo", "HOME": str(self.temporary)},
        }
        kwargs.update(overrides)
        return self.module.run_preflight(**kwargs)

    def test_no_op_control_stays_green(self) -> None:
        status = self.run_preflight()
        again = self.run_preflight(
            run_root=self.temporary / "run-noop",
            environ={
                "PATH": "/bin",
                "GH_TOKEN": "must-not-reach-cargo",
                "HOME": str(self.temporary),
                "ASSAY_UNUSED_NOOP": "1",
            },
        )
        self.assertEqual(status, 0)
        self.assertEqual(again, 0)

    def test_success_records_separate_cli_and_crate_provenance(self) -> None:
        self.assertEqual(self.run_preflight(), 0)
        cargo = self.world.command_argv("cargo install")
        self.assertEqual(
            cargo[:6],
            ["cargo", "install", "assay-mcp-server", "--version", "6.6.2", "--locked"],
        )
        self.assertIn("--root", cargo)
        self.assertNotIn("--path", cargo)
        opening = self.world.command_argv("published-release-platform-opening.sh")
        self.assertIn("--release-tag", opening)
        self.assertEqual(opening[opening.index("--release-tag") + 1], "v6.6.2")
        self.assertEqual(opening[opening.index("--target") + 1], "x86_64-pc-windows-msvc")
        results = self.run_root / "results"
        retained = retained_files(results)
        self.assertIn("cli-opening/results/attestation-verify.log", retained)
        self.assertEqual(
            (results / "cli-opening" / "results" / "attestation-verify.log").read_text(encoding="utf-8"),
            "attested\n",
        )
        crate = json.loads((results / "crate-provenance.json").read_text(encoding="utf-8"))
        cli = json.loads((results / "cli-provenance.json").read_text(encoding="utf-8"))
        identity = json.loads((results / "installed-mcp-identity.json").read_text(encoding="utf-8"))
        binary = self.run_root / "mcp-crate" / "bin" / "assay-mcp-server"
        self.assertTrue(REQUIRED_RETAINED <= retained, retained)
        self.assertEqual(cli["opening_results"], "cli-opening/results")
        self.assertFalse(Path(cli["opening_results"]).is_absolute())
        self.assertFalse((self.run_root / "cli-opening").exists())
        self.assertEqual(crate["kind"], "crates_io_api_declared_checksum")
        self.assertEqual(crate["checksum"], "ab" * 32)
        self.assertEqual(cli["kind"], "github_release_archive_attestation")
        self.assertNotIn("checksum", cli)
        self.assertNotEqual(cli["kind"], crate["kind"])
        self.assertEqual(identity["kind"], "installed_mcp_binary")
        self.assertEqual(identity["version"], "assay-mcp-server 6.6.2")
        self.assertEqual(identity["bytes"], binary.stat().st_size)
        self.assertEqual(identity["sha256"], hashlib.sha256(binary.read_bytes()).hexdigest())
        self.assertNotEqual(identity["kind"], crate["kind"])
        self.assertNotIn("origin", identity)
        cargo_env = next(call["env"] for call in self.world.calls if call.get("argv", [""])[0] == "cargo")
        self.assertNotIn("GH_TOKEN", cargo_env)

    def test_deny_wrong_id_fails(self) -> None:
        self.world.deny_stdout = (
            '{"jsonrpc":"2.0","id":1,"result":{}}\n'
            '{"jsonrpc":"2.0","id":10,"error":{"code":-31999,'
            '"data":{"origin":"assay-proxy","reason":"no_declared_allowance"}}}\n'
        )
        with self.assertRaisesRegex(self.module.PreflightError, "sent call"):
            self.run_preflight()

    def test_allow_wrong_id_fails(self) -> None:
        self.world.allow_stdout = (
            '{"jsonrpc":"2.0","id":1,"result":{}}\n'
            '{"jsonrpc":"2.0","id":10,"result":{"isError":false,"content":['
            '{"type":"text","text":"forwarded-ok (mock; no real GitHub call)"}]}}\n'
        )
        with self.assertRaisesRegex(self.module.PreflightError, "sent call"):
            self.run_preflight()

    def test_deny_missing_jsonrpc_fails(self) -> None:
        self.world.deny_stdout = (
            '{"jsonrpc":"2.0","id":1,"result":{}}\n'
            '{"id":9,"error":{"code":-31999,'
            '"data":{"origin":"assay-proxy","reason":"no_declared_allowance"}}}\n'
        )
        with self.assertRaisesRegex(self.module.PreflightError, "sent call"):
            self.run_preflight()

    def test_uppercase_api_checksum_fails(self) -> None:
        self.world.crate_checksum = "AB" * 32
        with self.assertRaisesRegex(self.module.PreflightError, "checksum"):
            self.run_preflight()

    def test_wrong_installed_version_fails(self) -> None:
        self.world.mcp_version = "assay-mcp-server 6.6.1"
        with self.assertRaisesRegex(self.module.PreflightError, "installed version"):
            self.run_preflight()

    def test_source_checkout_fallback_is_rejected(self) -> None:
        with self.assertRaisesRegex(self.module.PreflightError, "source checkout"):
            self.module.forbid_source_checkout(
                ["cargo", "install", "--path", "crates/assay-mcp-server", "--locked"]
            )
        self.world.place_binary_outside_root = True
        with self.assertRaisesRegex(self.module.PreflightError, "disposable root"):
            self.run_preflight()

    def test_mismatched_release_version_fails(self) -> None:
        with self.assertRaisesRegex(self.module.PreflightError, "mismatched release"):
            self.run_preflight(release_tag="v6.6.1")

    def test_child_failed_despite_parseable_stdout_fails(self) -> None:
        self.world.deny_status = 1
        with self.assertRaisesRegex(self.module.PreflightError, "child failed despite parseable stdout"):
            self.run_preflight()

    def test_allow_child_failed_despite_parseable_stdout_fails(self) -> None:
        self.world.allow_status = 1
        with self.assertRaisesRegex(self.module.PreflightError, "child failed despite parseable stdout"):
            self.run_preflight()

    def test_missing_denial_fails(self) -> None:
        self.world.deny_stdout = ALLOW_STDOUT
        with self.assertRaisesRegex(self.module.PreflightError, "denial"):
            self.run_preflight()

    def test_missing_allow_control_fails(self) -> None:
        self.world.allow_stdout = DENY_STDOUT
        with self.assertRaisesRegex(self.module.PreflightError, "allow"):
            self.run_preflight()

    def _call_for_script(self, filename: str) -> dict:
        for call in self.world.calls:
            argv = call.get("argv") or []
            if any(str(part).replace("\\", "/").endswith(filename) for part in argv):
                return call
        raise AssertionError(f"no recorded argv ended with {filename}")

    def test_reader_subprocess_boundary_uses_posix_path_sanitized_env_and_repo_cwd(self) -> None:
        repo = PureWindowsPath(r"D:\a\assay\assay")
        github_output = r"D:\a\_temp\github.output"
        self.assertEqual(self.run_preflight(
            repo_root=repo,
            environ={
                "PATH": r"C:\Windows\system32",
                "GH_TOKEN": "must-not-reach-reader",
                "GITHUB_TOKEN": "must-not-reach-reader",
                "GITHUB_OUTPUT": github_output,
                "HOME": str(self.temporary),
            },
        ), 0)
        reader = self._call_for_script("read-assay-release-tag.sh")
        expected_script = "D:/a/assay/assay/scripts/ci/read-assay-release-tag.sh"
        self.assertEqual(reader["argv"], ["bash", expected_script])
        self.assertNotIn("\\", reader["argv"][1])
        self.assertEqual(reader["cwd"], "D:/a/assay/assay")
        self.assertNotIn("GITHUB_OUTPUT", reader["env"])
        self.assertNotIn("GH_TOKEN", reader["env"])
        self.assertNotIn("GITHUB_TOKEN", reader["env"])
        self.assertEqual(reader["env"].get("PATH"), r"C:\Windows\system32")
        opening = self._call_for_script("published-release-platform-opening.sh")
        self.assertEqual(opening["argv"][0], "bash")
        self.assertEqual(
            opening["argv"][1],
            "D:/a/assay/assay/scripts/ci/published-release-platform-opening.sh",
        )
        self.assertNotIn("\\", opening["argv"][1])

    def test_failed_reader_retains_stdout_stderr_argv_and_rc(self) -> None:
        self.world.reader_status = 1
        self.world.reader_stdout = b"captured-stdout\n"
        self.world.reader_stderr = b""
        with self.assertRaisesRegex(self.module.PreflightError, "release tag reader failed with status 1"):
            self.run_preflight()
        results = self.run_root / "results"
        stdout_path = results / "release-tag-reader.stdout"
        stderr_path = results / "release-tag-reader.stderr"
        command_path = results / "release-tag-reader.command.json"
        self.assertTrue(stdout_path.is_file(), "reader stdout was not retained")
        self.assertTrue(stderr_path.is_file(), "reader stderr was not retained")
        self.assertTrue(command_path.is_file(), "reader argv/rc were not retained")
        self.assertEqual(stdout_path.read_bytes(), b"captured-stdout\n")
        self.assertEqual(stderr_path.read_bytes(), b"")
        command = json.loads((results / "release-tag-reader.command.json").read_text(encoding="utf-8"))
        self.assertEqual(command["returncode"], 1)
        self.assertEqual(command["argv"][0], "bash")
        self.assertTrue(str(command["argv"][1]).replace("\\", "/").endswith("read-assay-release-tag.sh"))
        self.assertFalse(any((call.get("argv") or [""])[0] == "cargo" for call in self.world.calls))
        status = json.loads((results / "status.json").read_text(encoding="utf-8"))
        self.assertEqual(status["status"], "fail")

    def _windows_environ(self, **extra: str) -> dict[str, str]:
        environ = {
            "PATH": r"C:\Windows\System32",
            "GH_TOKEN": "must-not-reach-reader",
            "GITHUB_TOKEN": "must-not-reach-reader",
            "HOME": str(self.temporary),
        }
        environ.update(extra)
        return environ

    def _assert_windows_git_bash(
        self,
        module,
        expected: str,
        *,
        existing: set[str] | None = None,
        extra_env: dict[str, str] | None = None,
    ) -> None:
        if existing is None:
            existing = {GIT_BASH, WSL_BASH}
        self._windows_runs = getattr(self, "_windows_runs", 0) + 1
        run_root = self.temporary / f"win-{self._windows_runs}"
        world = FakeWorld(module, run_root)
        with patch.object(sys, "platform", "win32"), patch.object(os.path, "isfile", scripted_isfile(existing)):
            status = module.run_preflight(
                run_root=run_root,
                repo_root=ROOT,
                runner=world.run,
                download=world.download,
                environ=self._windows_environ(**(extra_env or {})),
            )
        self.assertEqual(status, 0)
        for filename in ("read-assay-release-tag.sh", "published-release-platform-opening.sh"):
            argv = argv_for(world, filename)
            self.assertEqual(argv[0], expected, filename)
            self.assertNotIn("\\", argv[1])
            self.assertTrue(str(argv[1]).replace("\\", "/").endswith(filename), argv)

    def test_windows_resolution_ambiguity_selects_checked_git_bash_for_both_children(self) -> None:
        self._assert_windows_git_bash(self.module, GIT_BASH)
        self._assert_windows_git_bash(
            self.module,
            GIT_BASH_USR,
            existing={GIT_BASH_USR, WSL_BASH},
        )

    def test_workflow_supplied_bash_is_used_when_it_validates(self) -> None:
        self._assert_windows_git_bash(
            self.module,
            SUPPLIED_BASH,
            existing={SUPPLIED_BASH, GIT_BASH, WSL_BASH},
            extra_env={"ASSAY_WINDOWS_BASH": SUPPLIED_BASH},
        )

    def test_unavailable_intended_windows_bash_fails_closed(self) -> None:
        cases = {
            "only-wsl": ({WSL_BASH}, {}),
            "supplied-missing": (
                {GIT_BASH, WSL_BASH},
                {"ASSAY_WINDOWS_BASH": r"D:\absent\Git\bin\bash.exe"},
            ),
            "supplied-wsl": ({GIT_BASH, WSL_BASH}, {"ASSAY_WINDOWS_BASH": WSL_BASH}),
            "supplied-relative": (
                {GIT_BASH, r"Git\bin\bash.exe"},
                {"ASSAY_WINDOWS_BASH": r"Git\bin\bash.exe"},
            ),
        }
        for label, (existing, extra) in cases.items():
            with self.subTest(label=label):
                run_root = self.temporary / label
                world = FakeWorld(self.module, run_root)
                with patch.object(sys, "platform", "win32"), patch.object(
                    os.path, "isfile", scripted_isfile(existing)
                ):
                    with self.assertRaisesRegex(
                        self.module.PreflightError,
                        "Windows Git Bash executable is unavailable",
                    ):
                        self.module.run_preflight(
                            run_root=run_root,
                            repo_root=ROOT,
                            runner=world.run,
                            download=world.download,
                            environ=self._windows_environ(**extra),
                        )
                joined = " ".join(
                    " ".join(str(part) for part in (call.get("argv") or [])) for call in world.calls
                )
                self.assertNotIn("read-assay-release-tag.sh", joined)
                self.assertNotIn("published-release-platform-opening.sh", joined)

    def test_posix_child_invocations_keep_bare_bash(self) -> None:
        self.assertNotEqual(sys.platform, "win32")
        self.assertEqual(
            self.run_preflight(
                environ={
                    "PATH": "/bin",
                    "GH_TOKEN": "must-not-reach-reader",
                    "HOME": str(self.temporary),
                    "ASSAY_WINDOWS_BASH": SUPPLIED_BASH,
                }
            ),
            0,
        )
        for filename in ("read-assay-release-tag.sh", "published-release-platform-opening.sh"):
            argv = argv_for(self.world, filename)
            self.assertEqual(argv[0], "bash", filename)

    def test_windows_reader_failure_retains_explicit_argv_output_and_rc(self) -> None:
        stdout = "Windows Subsystem for Linux has no installed distributions.\r\n".encode("utf-16-le")
        self.world.reader_status = 1
        self.world.reader_stdout = stdout
        self.world.reader_stderr = b""
        with patch.object(sys, "platform", "win32"), patch.object(
            os.path, "isfile", scripted_isfile({GIT_BASH, WSL_BASH})
        ):
            with self.assertRaisesRegex(self.module.PreflightError, "release tag reader failed with status 1"):
                self.run_preflight(environ=self._windows_environ())
        results = self.run_root / "results"
        self.assertEqual((results / "release-tag-reader.stdout").read_bytes(), stdout)
        self.assertEqual((results / "release-tag-reader.stderr").read_bytes(), b"")
        command = json.loads((results / "release-tag-reader.command.json").read_text(encoding="utf-8"))
        self.assertEqual(command["returncode"], 1)
        self.assertEqual(command["argv"][0], GIT_BASH)
        self.assertTrue(str(command["argv"][1]).replace("\\", "/").endswith("read-assay-release-tag.sh"))
        self.assertFalse(any((call.get("argv") or [""])[0] == "cargo" for call in self.world.calls))

    def test_isolated_mutation_copy_bypasses_explicit_executable(self) -> None:
        source = PREFLIGHT.read_text(encoding="utf-8")
        explicit = "return [executable, script.as_posix()]"
        noop = load_preflight_source(self.temporary / "noop-preflight.py", source + "\n")
        self._assert_windows_git_bash(noop, GIT_BASH)
        bypass_source = source.replace(explicit, 'return ["bash", script.as_posix()]', 1)
        self.assertNotEqual(bypass_source, source)
        bypass = load_preflight_source(self.temporary / "bypass-preflight.py", bypass_source)
        with self.assertRaises(AssertionError):
            self._assert_windows_git_bash(bypass, GIT_BASH)


class WorkflowContractTests(unittest.TestCase):
    def test_dispatch_only_minimum_pinned_surface(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        self.assertIn("on:\n  workflow_dispatch:\n", text)
        self.assertNotIn("pull_request:", text)
        self.assertNotIn("push:", text)
        self.assertNotIn("schedule:", text)
        self.assertIn("permissions:\n  contents: read\n", text)
        self.assertNotIn("id-token:", text)
        self.assertIn("windows-latest", text)
        self.assertIn("scripts/ci/published-native-mcp-preflight.py", text)
        self.assertIn("if: always()", text)
        self.assertNotIn("v6.6.2", text)
        self.assertNotIn("toolchain:", text)
        for pin in PINNED_ACTIONS:
            self.assertIn(pin, text)
        uses = [line.strip() for line in text.splitlines() if line.strip().startswith("uses:")]
        self.assertEqual(len(uses), 3)
        self.assertTrue(all(any(pin in line for pin in PINNED_ACTIONS) for line in uses))

    def test_exercise_step_hands_off_windows_bash_executable(self) -> None:
        text = WORKFLOW.read_text(encoding="utf-8")
        exercise = text.split("name: Exercise the published crate MCP preflight", 1)[1]
        exercise = exercise.split("name: Retain preflight evidence", 1)[0]
        self.assertIn('ASSAY_WINDOWS_BASH="$(cygpath -w -- "$BASH")"', exercise)
        self.assertIn("export ASSAY_WINDOWS_BASH", exercise)
        self.assertNotIn("wsl", exercise.casefold())

    def test_ci_scope_wires_lightweight_suite(self) -> None:
        require_native_preflight_ci_wiring(CI_WORKFLOW.read_text(encoding="utf-8"))

    def test_inert_ci_invocations_are_not_active_wiring(self) -> None:
        live = CI_WORKFLOW.read_text(encoding="utf-8")
        require_native_preflight_ci_wiring(live)
        command_line = f"          {CI_SCOPE_COMMAND}\n"
        heading = f"      - name: {CI_SCOPE_STEP}\n"
        mutations = {
            "deleted": live.replace(command_line, "", 1),
            "comment-only": live.replace(command_line, f"          # {CI_SCOPE_COMMAND}\n", 1),
            "if:false": live.replace(heading, heading + "        if: false\n", 1),
        }
        for label, mutated in mutations.items():
            with self.subTest(label=label):
                self.assertNotEqual(mutated, live, label)
                with self.assertRaises(AssertionError):
                    require_native_preflight_ci_wiring(mutated)
        require_native_preflight_ci_wiring(live)


def _job_block(text: str, job: str) -> str:
    match = re.search(
        rf"(?ms)^  {re.escape(job)}:\n.*?(?=^  [A-Za-z0-9_-]+:\s*$|\Z)",
        text,
    )
    if match is None:
        raise AssertionError(f"ci.yml has no {job} job")
    return match.group(0)


def _step_block(job: str, name: str) -> str:
    matches = list(
        re.finditer(
            rf"(?ms)^      - name: {re.escape(name)}\s*$\n.*?(?=^      - (?:name:|uses:)|\Z)",
            job,
        )
    )
    if len(matches) != 1:
        raise AssertionError(f"expected one {name!r} step, found {len(matches)}")
    return matches[0].group(0)


def _direct_step_keys(step: str) -> dict[str, str]:
    keys: dict[str, str] = {}
    for raw in step.splitlines():
        if raw.startswith("      - name:"):
            value = raw.split(":", 1)[1].strip()
            if "name" in keys:
                raise AssertionError("duplicate step key name")
            keys["name"] = value
            continue
        if raw.startswith("        ") and not raw.startswith("          "):
            stripped = raw.strip()
            if not stripped or stripped.startswith("#"):
                continue
            key, _, value = stripped.partition(":")
            if key in keys:
                raise AssertionError(f"duplicate step key {key}")
            keys[key] = value.strip()
    return keys


def _active_run_lines(step: str) -> list[str]:
    active: list[str] = []
    in_run = False
    for raw in step.splitlines():
        if raw == "        run: |":
            in_run = True
            continue
        if in_run:
            if raw.strip() and not raw.startswith("          "):
                break
            stripped = raw.strip()
            if not stripped or stripped.startswith("#"):
                continue
            active.append(stripped)
    return active


def require_native_preflight_ci_wiring(text: str) -> None:
    # Reads caller-supplied workflow text only. The suite file is not an oracle.
    step = _step_block(_job_block(text, "scope"), CI_SCOPE_STEP)
    keys = _direct_step_keys(step)
    if set(keys) != {"name", "shell", "run"}:
        raise AssertionError(f"step keys {sorted(keys)} are not the closed map")
    if keys["shell"] != "bash" or keys["run"] != "|":
        raise AssertionError("step must be an unconditional bash block")
    active = tuple(_active_run_lines(step))
    if active != CI_SCOPE_ACTIVE:
        raise AssertionError(f"active run lines {active} are not the required command")
    if not re.search(r"(?m)^    needs: \[scope,", _job_block(text, "ci")):
        raise AssertionError("aggregator does not already wait on scope")


if __name__ == "__main__":
    os.environ.pop("PR_HEAD_SHA", None)
    unittest.main()
