#!/usr/bin/env python3
"""Execute and record the published-release proxy phase from one argv value."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource
import shutil
import signal
import subprocess
import sys


MAX_REQUEST_BYTES = 1_048_576
MAX_OUTPUT_BYTES = 16_777_216


def bounded_seconds(value: str) -> int:
    seconds = int(value)
    if not 1 <= seconds <= 300:
        raise argparse.ArgumentTypeError("timeout must be between 1 and 300 seconds")
    return seconds


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--timeout-seconds", type=bounded_seconds, default=60)
    return parser.parse_args()


def append_command_record(path: Path, exit_code: int, argv: list[str]) -> None:
    record = {"name": "proxy-enforce", "exit_code": exit_code, "argv": argv}
    with path.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
        handle.flush()
        os.fsync(handle.fileno())


def child_environment() -> dict[str, str]:
    allowed = ("HOME", "PATH", "LANG", "LC_ALL", "TZ")
    return {key: os.environ[key] for key in allowed if key in os.environ}


def limit_child_output() -> None:
    resource.setrlimit(resource.RLIMIT_FSIZE, (MAX_OUTPUT_BYTES, MAX_OUTPUT_BYTES))


def stop_process_group(process: subprocess.Popen[bytes]) -> None:
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=1)
    except subprocess.TimeoutExpired:
        pass
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    process.wait()


def main() -> int:
    args = parse_args()
    results = Path.cwd().resolve()
    harness_root = Path(__file__).resolve().parents[2]
    fixture_root = harness_root / "examples/privileged-action-gate"
    decisions = results / "decisions.ndjson"
    observations = results / "denied-observations.ndjson"
    request = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(request) > MAX_REQUEST_BYTES:
        raise SystemExit("proxy request exceeds 1 MiB ceiling")

    mcp_bin = shutil.which("assay-mcp-server", path=child_environment().get("PATH"))
    if mcp_bin is None:
        raise SystemExit("assay-mcp-server is absent from the restricted PATH")

    argv = [
        mcp_bin,
        "proxy-enforce",
        "--upstream-command",
        sys.executable,
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        str(fixture_root / "mock_github_mcp.py"),
        "--enforce-policy",
        str(fixture_root / "policies/no-allowance.yaml"),
        "--declared-mcp-manifest",
        str(fixture_root / "baseline-approved.json"),
        "--enforcement-decision-out",
        str(decisions),
        "--denied-call-observation-out",
        str(observations),
    ]
    with (results / "proxy.jsonl").open("wb") as stdout_handle, (
        results / "proxy.stderr"
    ).open("wb") as stderr_handle:
        try:
            process = subprocess.Popen(
                argv,
                stdin=subprocess.PIPE,
                stdout=stdout_handle,
                stderr=stderr_handle,
                env=child_environment(),
                preexec_fn=limit_child_output,
                start_new_session=True,
            )
            try:
                process.communicate(input=request, timeout=args.timeout_seconds)
                status = process.returncode
            except subprocess.TimeoutExpired:
                stop_process_group(process)
                status = 124
        except OSError as error:
            stderr_handle.write(f"proxy process failed to start: {error}\n".encode())
            status = 127
    append_command_record(results / "commands.ndjson", status, argv)
    return status if 0 <= status <= 255 else 125


if __name__ == "__main__":
    raise SystemExit(main())
