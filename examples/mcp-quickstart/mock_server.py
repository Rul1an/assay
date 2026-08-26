#!/usr/bin/env python3
"""Bounded local MCP fixture for the released quickstart. No network or real effect."""

from __future__ import annotations

import json
import sys
from pathlib import Path

MAX_LINE_BYTES = 65_536
TOOL = {
    "name": "read_file",
    "description": "Read one file from the local demo directory",
    "inputSchema": {
        "type": "object",
        "additionalProperties": False,
        "properties": {"path": {"type": "string"}},
        "required": ["path"],
    },
}


def send(value: dict) -> None:
    sys.stdout.write(json.dumps(value, separators=(",", ":")) + "\n")
    sys.stdout.flush()


def record_invocation() -> None:
    Path(".assay/quickstart/mock-invocation.json").write_text(
        json.dumps({"argv": sys.argv[1:], "cwd": str(Path.cwd())}, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def main() -> int:
    record_invocation()
    while True:
        raw = sys.stdin.buffer.readline(MAX_LINE_BYTES + 1)
        if not raw:
            return 0
        if len(raw) > MAX_LINE_BYTES or not raw.endswith(b"\n"):
            return 2
        try:
            request = json.loads(raw)
        except (UnicodeDecodeError, json.JSONDecodeError):
            return 2
        method = request.get("method")
        request_id = request.get("id")
        if method == "initialize":
            result = {
                "protocolVersion": "2024-11-05",
                "capabilities": {"tools": {}},
                "serverInfo": {"name": "assay-quickstart-mock", "version": "1"},
            }
        elif method == "tools/list":
            result = {"tools": [TOOL]}
        elif method == "tools/call":
            result = {
                "content": [{"type": "text", "text": "mock-read-ok; no external effect"}],
                "isError": False,
            }
        else:
            send({"jsonrpc": "2.0", "id": request_id, "error": {"code": -32601, "message": "method not found"}})
            continue
        send({"jsonrpc": "2.0", "id": request_id, "result": result})


if __name__ == "__main__":
    raise SystemExit(main())
