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
from pathlib import Path
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PREFLIGHT = ROOT / "scripts/ci/published-native-mcp-preflight.py"
WORKFLOW = ROOT / ".github/workflows/published-native-mcp-preflight.yml"
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
REQUIRED_RETAINED = {
    "cli-opening/results/attestation-verify.log",
    "cli-provenance.json",
    "crate-provenance.json",
    "installed-mcp-identity.json",
    "status.json",
}


def retained_files(results: Path) -> set[str]:
    return {str(path.relative_to(results)) for path in results.rglob("*") if path.is_file()}


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
            return self.module.CommandResult(0, self.pin.encode(), b"", list(argv))
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

    def test_missing_denial_fails(self) -> None:
        self.world.deny_stdout = ALLOW_STDOUT
        with self.assertRaisesRegex(self.module.PreflightError, "denial"):
            self.run_preflight()

    def test_missing_allow_control_fails(self) -> None:
        self.world.allow_stdout = DENY_STDOUT
        with self.assertRaisesRegex(self.module.PreflightError, "allow"):
            self.run_preflight()


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


if __name__ == "__main__":
    os.environ.pop("PR_HEAD_SHA", None)
    unittest.main()
