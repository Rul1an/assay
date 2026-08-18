#!/usr/bin/env python3
"""Execute and record the published-release proxy phase from one argv value."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import resource
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
    parser.add_argument("--mcp-bin", required=True)
    parser.add_argument("--python-bin", required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--policy", type=Path, required=True)
    parser.add_argument("--declared-manifest", type=Path, required=True)
    parser.add_argument("--decisions", type=Path, required=True)
    parser.add_argument("--observations", type=Path, required=True)
    parser.add_argument("--commands", type=Path, required=True)
    parser.add_argument("--stdout", type=Path, required=True)
    parser.add_argument("--stderr", type=Path, required=True)
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


def main() -> int:
    args = parse_args()
    request = sys.stdin.buffer.read(MAX_REQUEST_BYTES + 1)
    if len(request) > MAX_REQUEST_BYTES:
        raise SystemExit("proxy request exceeds 1 MiB ceiling")

    argv = [
        args.mcp_bin,
        "proxy-enforce",
        "--upstream-command",
        args.python_bin,
        "--upstream-arg",
        "-u",
        "--upstream-arg",
        str(args.fixture),
        "--enforce-policy",
        str(args.policy),
        "--declared-mcp-manifest",
        str(args.declared_manifest),
        "--enforcement-decision-out",
        str(args.decisions),
        "--denied-call-observation-out",
        str(args.observations),
    ]
    with args.stdout.open("wb") as stdout_handle, args.stderr.open("wb") as stderr_handle:
        try:
            completed = subprocess.run(
                argv,
                input=request,
                stdout=stdout_handle,
                stderr=stderr_handle,
                check=False,
                env=child_environment(),
                preexec_fn=limit_child_output,
                timeout=args.timeout_seconds,
            )
            status = completed.returncode
        except subprocess.TimeoutExpired:
            status = 124
        except OSError as error:
            stderr_handle.write(f"proxy process failed to start: {error}\n".encode())
            status = 127
    append_command_record(args.commands, status, argv)
    return status if 0 <= status <= 255 else 125


if __name__ == "__main__":
    raise SystemExit(main())
