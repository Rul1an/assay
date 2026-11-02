#!/usr/bin/env python3
"""Synthetic legacy-elicitation client, bounded by run_producer's process group.

Uses the same server fixture as the Rust ask/answer tests. No hosted model,
credentials, network or real action. The parent retains stdout/stderr and kills
the complete owned group on timeout, including a stuck wrap/server child.
"""
import json
from pathlib import Path
import subprocess
import sys


def main():
    binary, server, action = sys.argv[1:]
    if action not in ("accept", "decline"):
        raise ValueError("expected accept or decline")
    command = [binary, "mcp", "wrap", "--policy", "policy.yaml",
               "--event-source", "assay://offline-mixed", "--decision-log", "decisions.ndjson",
               "--decision-log-new", "--request-receipts-out", "receipts.ndjson",
               "--", sys.executable, "-I", server, "legacy_ask"]
    with Path("requests.ndjson").open("xb") as sent, subprocess.Popen(
            command, stdin=subprocess.PIPE, stdout=subprocess.PIPE) as process:
        def send(value):
            raw = (json.dumps(value, separators=(",", ":")) + "\n").encode()
            sent.write(raw)
            sent.flush()
            process.stdin.write(raw)
            process.stdin.flush()

        def receive():
            raw = process.stdout.readline(65_537)
            if not raw.endswith(b"\n") or len(raw) > 65_536:
                raise ValueError("missing or oversized synthetic response")
            sys.stdout.buffer.write(raw)
            sys.stdout.buffer.flush()
            return json.loads(raw)

        send({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
            "protocolVersion": "2025-11-25", "capabilities": {"elicitation": {}},
            "clientInfo": {"name": "offline-mixed", "version": "1"}}})
        if receive().get("id") != 1:
            raise ValueError("unexpected initialize response")
        send({"jsonrpc": "2.0", "id": 2, "method": "tools/call",
              "params": {"name": "del", "arguments": {}}})
        ask = receive()
        if ask.get("method") != "elicitation/create" or ask.get("id") != "ask-1":
            raise ValueError("missing expected server ask")
        send({"jsonrpc": "2.0", "id": "ask-1", "result": {"action": action}})
        if receive().get("id") != 2:
            raise ValueError("missing tool result after answer")
        process.stdin.close()
        if process.wait(timeout=5) != 0:
            raise ValueError("wrap failed")
        process.stdout.close()


if __name__ == "__main__":
    main()
