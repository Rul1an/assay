#!/usr/bin/env python3
"""Dispatch-only Windows preflight for the published crates.io MCP route.

Reads the checked-in release pin, calls the attested CLI opening helper, installs
assay-mcp-server from crates.io into a disposable prefix, and crosses one
documented proxy-enforce denial plus allow control. Not a launch pass.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import sys
from typing import Callable


ROOT = Path(__file__).resolve().parents[2]
TARGET = "x86_64-pc-windows-msvc"
CRATE = "assay-mcp-server"
TOKEN_KEYS = ("GH_TOKEN", "GITHUB_TOKEN")
FORWARD_OK = "forwarded-ok (mock; no real GitHub call)"
API_CHECKSUM = re.compile(r"^[0-9a-f]{64}$")
FIXTURE = Path("examples/privileged-action-gate")
INIT_REQUEST = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "initialize",
    "params": {
        "protocolVersion": "2024-11-05",
        "capabilities": {},
        "clientInfo": {"name": "published-native-mcp-preflight", "version": "1"},
    },
}
CALL_REQUEST = {
    "jsonrpc": "2.0",
    "id": 9,
    "method": "tools/call",
    "params": {"name": "github.add_deploy_key", "arguments": {"owner": "acme", "repo": "prod-app"}},
}


class PreflightError(Exception):
    pass


class CommandResult:
    def __init__(self, returncode: int, stdout: bytes, stderr: bytes, argv: list[str]) -> None:
        self.returncode = returncode
        self.stdout = stdout
        self.stderr = stderr
        self.argv = argv


def default_run(argv, **kwargs) -> CommandResult:
    try:
        completed = subprocess.run(
            list(argv),
            cwd=kwargs.get("cwd"),
            env=kwargs.get("env"),
            input=kwargs.get("input"),
            timeout=kwargs.get("timeout"),
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        return CommandResult(124, error.stdout or b"", error.stderr or b"", list(argv))
    except OSError as error:
        return CommandResult(127, b"", str(error).encode(), list(argv))
    return CommandResult(completed.returncode, completed.stdout, completed.stderr, list(argv))


def forbid_source_checkout(argv: list[str]) -> None:
    if "--path" in argv or any("crates/assay-mcp-server" in str(part).replace("\\", "/") for part in argv):
        raise PreflightError("source checkout fallback is forbidden")


def cargo_install_argv(version: str, root: Path) -> list[str]:
    argv = ["cargo", "install", CRATE, "--version", version, "--locked", "--root", str(root)]
    forbid_source_checkout(argv)
    return argv


def crate_metadata_url(version: str) -> str:
    return f"https://crates.io/api/v1/crates/{CRATE}/{version}"


def without_tokens(env: dict[str, str]) -> dict[str, str]:
    return {key: value for key, value in env.items() if key not in TOKEN_KEYS}


def bash_script_argv(script: Path) -> list[str]:
    return ["bash", script.as_posix()]


def reader_environ(env: dict[str, str]) -> dict[str, str]:
    cleaned = without_tokens(env)
    cleaned.pop("GITHUB_OUTPUT", None)
    return cleaned


def retain_command(results: Path, name: str, result: CommandResult) -> None:
    write_bytes(results / f"{name}.stdout", result.stdout)
    write_bytes(results / f"{name}.stderr", result.stderr)
    write_json(
        results / f"{name}.command.json",
        {"argv": result.argv, "returncode": result.returncode},
    )


def require_status(result: CommandResult, label: str) -> None:
    if result.returncode != 0:
        detail = result.stderr.decode("utf-8", errors="replace")[-2000:]
        raise PreflightError(f"{label} failed with status {result.returncode}: {detail}")


def parse_json_lines(stdout: bytes) -> list[dict]:
    records = []
    for raw in stdout.splitlines():
        if not raw.strip():
            continue
        try:
            parsed = json.loads(raw)
        except json.JSONDecodeError as error:
            raise PreflightError("proxy-enforce stdout was not JSON lines") from error
        if not isinstance(parsed, dict):
            raise PreflightError("proxy-enforce stdout contained a non-object")
        records.append(parsed)
    return records


def require_call_reply(replies: list[dict]) -> dict:
    reply = replies[-1] if replies else {}
    if reply.get("jsonrpc") != CALL_REQUEST["jsonrpc"] or reply.get("id") != CALL_REQUEST["id"]:
        raise PreflightError("proxy-enforce reply did not correlate with the sent call")
    return reply


def assert_proxy_denial(status: int, stdout: bytes) -> None:
    replies = parse_json_lines(stdout)
    if status != 0:
        if replies:
            raise PreflightError("proxy-enforce child failed despite parseable stdout")
        raise PreflightError(f"proxy-enforce child failed with status {status}")
    error = require_call_reply(replies).get("error")
    data = error.get("data") if isinstance(error, dict) else None
    if not (
        isinstance(error, dict)
        and error.get("code") == -31999
        and isinstance(data, dict)
        and data.get("origin") == "assay-proxy"
        and data.get("reason") == "no_declared_allowance"
    ):
        raise PreflightError("proxy-enforce denial contract was missing")


def assert_proxy_allow(status: int, stdout: bytes) -> None:
    replies = parse_json_lines(stdout)
    if status != 0:
        if replies:
            raise PreflightError("proxy-enforce allow child failed despite parseable stdout")
        raise PreflightError(f"proxy-enforce allow child failed with status {status}")
    result = require_call_reply(replies).get("result")
    if not (
        isinstance(result, dict)
        and result.get("isError") is False
        and result.get("content") == [{"type": "text", "text": FORWARD_OK}]
    ):
        raise PreflightError("proxy-enforce allow control was missing")


def installed_mcp_binary(root: Path) -> Path:
    resolved_root = root.resolve()
    for name in ("assay-mcp-server.exe", "assay-mcp-server"):
        candidate = root / "bin" / name
        if not candidate.is_file():
            continue
        resolved = candidate.resolve()
        try:
            resolved.relative_to(resolved_root)
        except ValueError as error:
            raise PreflightError("installed assay-mcp-server is outside the disposable root") from error
        return resolved
    raise PreflightError("installed assay-mcp-server is missing from the disposable root")


def write_json(path: Path, payload: dict) -> None:
    path.write_text(json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_bytes(path: Path, data: bytes) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(data)


def proxy_argv(mcp_bin: Path, policy: Path) -> list[str]:
    return [
        str(mcp_bin),
        "proxy-enforce",
        "--upstream-command",
        sys.executable,
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        "mock_github_mcp.py",
        "--enforce-policy",
        str(policy),
        "--declared-mcp-manifest",
        "baseline-approved.json",
    ]


def run_preflight(
    *,
    run_root: Path,
    repo_root: Path,
    runner: Callable[..., CommandResult],
    download: Callable[..., None],
    environ: dict[str, str],
    release_tag: str | None = None,
) -> int:
    if run_root.exists():
        raise PreflightError(f"run root already exists: {run_root}")
    run_root.mkdir(parents=True)
    results = run_root / "results"
    results.mkdir()
    try:
        reader = runner(
            bash_script_argv(repo_root / "scripts/ci/read-assay-release-tag.sh"),
            cwd=repo_root.as_posix(),
            env=reader_environ(environ),
            timeout=30,
        )
        retain_command(results, "release-tag-reader", reader)
        require_status(reader, "release tag reader")
        pin = reader.stdout.decode("utf-8").strip().splitlines()[0]
        if release_tag is not None and release_tag != pin:
            raise PreflightError(f"mismatched release version: pin is {pin}, requested {release_tag}")
        version = pin[1:]
        opening_root = results / "cli-opening"
        opening = runner(
            [
                *bash_script_argv(repo_root / "scripts/ci/published-release-platform-opening.sh"),
                "--release-tag",
                pin,
                "--target",
                TARGET,
                "--run-root",
                str(opening_root),
            ],
            env=dict(environ),
            timeout=300,
        )
        write_bytes(results / "opening.stdout", opening.stdout)
        write_bytes(results / "opening.stderr", opening.stderr)
        require_status(opening, "published CLI opening")
        write_json(
            results / "cli-provenance.json",
            {
                "kind": "github_release_archive_attestation",
                "opening_results": "cli-opening/results",
                "release_tag": pin,
                "target": TARGET,
            },
        )
        metadata_path = results / "crate-metadata.json"
        download(crate_metadata_url(version), metadata_path, max_bytes=1_048_576)
        version_obj = json.loads(metadata_path.read_text(encoding="utf-8")).get("version")
        if not isinstance(version_obj, dict) or version_obj.get("num") != version:
            raise PreflightError("mismatched release version in crate metadata")
        if version_obj.get("yanked") is not False:
            raise PreflightError("crate version is yanked")
        checksum = version_obj.get("checksum")
        if not isinstance(checksum, str) or API_CHECKSUM.fullmatch(checksum) is None:
            raise PreflightError("crate checksum is not 64 lowercase hex")
        write_json(
            results / "crate-provenance.json",
            {
                "kind": "crates_io_api_declared_checksum",
                "crate": CRATE,
                "version": version,
                "checksum": checksum,
                "yanked": False,
                "url": crate_metadata_url(version),
            },
        )
        install_root = run_root / "mcp-crate"
        cargo = runner(cargo_install_argv(version, install_root), env=without_tokens(environ), timeout=3000)
        write_bytes(results / "cargo-install.stdout", cargo.stdout)
        write_bytes(results / "cargo-install.stderr", cargo.stderr)
        require_status(cargo, "cargo install")
        mcp_bin = installed_mcp_binary(install_root)
        version_result = runner([str(mcp_bin), "--version"], env=without_tokens(environ), timeout=30)
        write_bytes(results / "mcp-version.txt", version_result.stdout)
        require_status(version_result, "assay-mcp-server --version")
        got = version_result.stdout.decode("utf-8").strip()
        expected = f"{CRATE} {version}"
        if got != expected:
            raise PreflightError(f"wrong installed version: expected {expected}, got {got}")
        payload = mcp_bin.read_bytes()
        write_json(
            results / "installed-mcp-identity.json",
            {
                "kind": "installed_mcp_binary",
                "crate": CRATE,
                "filename": mcp_bin.name,
                "sha256": hashlib.sha256(payload).hexdigest(),
                "bytes": len(payload),
                "version": got,
            },
        )
        request = (json.dumps(INIT_REQUEST) + "\n" + json.dumps(CALL_REQUEST) + "\n").encode()
        fixture_root = repo_root / FIXTURE
        deny = runner(
            proxy_argv(mcp_bin, Path("policies/no-allowance.yaml")),
            cwd=fixture_root,
            env=without_tokens(environ),
            input=request,
            timeout=60,
        )
        write_bytes(results / "deny/proxy.jsonl", deny.stdout)
        write_bytes(results / "deny/proxy.stderr", deny.stderr)
        assert_proxy_denial(deny.returncode, deny.stdout)
        allow = runner(
            proxy_argv(mcp_bin, Path("policies/allow.yaml")),
            cwd=fixture_root,
            env=without_tokens(environ),
            input=request,
            timeout=60,
        )
        write_bytes(results / "allow/proxy.jsonl", allow.stdout)
        write_bytes(results / "allow/proxy.stderr", allow.stderr)
        assert_proxy_allow(allow.returncode, allow.stdout)
        write_json(results / "status.json", {"status": "ok", "release_tag": pin, "target": TARGET})
        return 0
    except Exception as error:
        write_json(results / "status.json", {"status": "fail", "detail": str(error)})
        raise


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--run-root", required=True)
    parser.add_argument("--release-tag")
    parser.add_argument("--repo-root", default=str(ROOT))
    args = parser.parse_args(argv)
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    from bounded_download import download

    try:
        return run_preflight(
            run_root=Path(args.run_root),
            repo_root=Path(args.repo_root),
            runner=default_run,
            download=download,
            environ=os.environ.copy(),
            release_tag=args.release_tag,
        )
    except PreflightError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
