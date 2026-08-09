#!/usr/bin/env python3
"""Prove the bundled mock discards oversized records without losing the stream."""

from __future__ import annotations

import json
from pathlib import Path
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[2]
MOCK = ROOT / "examples/privileged-action-gate/mock_github_mcp.py"
MAX_REQUEST_BYTES = 1_048_576


def main() -> None:
    oversized = {
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {"padding": "x" * MAX_REQUEST_BYTES},
    }
    ping = {"jsonrpc": "2.0", "id": 2, "method": "ping"}
    stdin = (
        json.dumps(oversized, separators=(",", ":"))
        + "\n"
        + json.dumps(ping, separators=(",", ":"))
        + "\n"
    ).encode("utf-8")
    try:
        result = subprocess.run(
            [sys.executable, str(MOCK)],
            input=stdin,
            cwd=MOCK.parent,
            capture_output=True,
            timeout=5,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise SystemExit(f"mock server did not bound oversized request handling: {error}")
    if result.returncode != 0:
        raise SystemExit(
            f"mock server rejected the bounded follow-up probe: {result.stderr[-300:]!r}"
        )
    response_ids = [json.loads(raw).get("id") for raw in result.stdout.splitlines()]
    if response_ids != [2]:
        raise SystemExit(
            "mock server must discard an oversized request and continue at the next "
            f"newline-delimited request; got response ids {response_ids}"
        )
    print("golden-path mock request bound: pass")


if __name__ == "__main__":
    main()
