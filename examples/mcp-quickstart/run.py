#!/usr/bin/env python3
"""Run one bounded, local, byte-recorded Assay MCP quickstart."""

from __future__ import annotations

import json
import os
import shutil
import signal
import subprocess
import sys
from pathlib import Path

REQUESTS = (
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2024-11-05", "capabilities": {}, "clientInfo": {"name": "assay-quickstart", "version": "1"}}},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {"name": "read_file", "arguments": {"path": "/tmp/assay-demo/safe.txt"}}},
)


def fail(message: str) -> int:
    print(f"quickstart failed: {message}", file=sys.stderr)
    return 1


def parse_timeout() -> float:
    raw = os.environ.get("ASSAY_QUICKSTART_TIMEOUT_SECONDS", "15")
    try:
        value = float(raw)
    except ValueError as error:
        raise ValueError("timeout must be numeric") from error
    if not 1 <= value <= 60:
        raise ValueError("timeout must be between 1 and 60 seconds")
    return value


def terminate(proc: subprocess.Popen[bytes]) -> None:
    if os.name == "posix":
        os.killpg(proc.pid, signal.SIGKILL)
    else:
        proc.kill()
    proc.wait(timeout=5)


def resolve_assay() -> str | None:
    """Return the released command name without accepting a command override."""
    return "assay" if shutil.which("assay") is not None else None


def main() -> int:
    source = Path(__file__).resolve().parent
    policy = source / "policy.yaml"
    mock = source / "mock_server.py"
    if not policy.is_file() or not mock.is_file():
        return fail("release quickstart assets are incomplete")

    assay = resolve_assay()
    if not assay:
        return fail("assay is not on PATH; install the released CLI first")
    try:
        timeout = parse_timeout()
    except ValueError as error:
        return fail(str(error))

    evidence = Path.cwd() / ".assay/quickstart"
    if evidence.exists():
        return fail(f"refusing to mix with an existing artifact directory: {evidence}")
    evidence.mkdir(parents=True)
    decision_log = evidence / "decisions.ndjson"
    invocation_log = evidence / "mock-invocation.json"
    child_stdout = evidence / "mcp.stdout.ndjson"
    child_stderr = evidence / "mcp.stderr.txt"

    command = [
        assay,
        "mcp",
        "wrap",
        "--policy",
        str(policy),
        "--verbose",
        "--event-source",
        "assay://quickstart/local-mock",
        "--decision-log",
        str(decision_log),
        "--",
        sys.executable,
        "-u",
        str(mock),
    ]
    creationflags = subprocess.CREATE_NEW_PROCESS_GROUP if os.name == "nt" else 0
    proc = subprocess.Popen(
        command,
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=os.name == "posix",
        creationflags=creationflags,
    )
    request_bytes = b"".join(
        json.dumps(request, separators=(",", ":")).encode("utf-8") + b"\n" for request in REQUESTS
    )
    try:
        stdout, stderr = proc.communicate(request_bytes, timeout=timeout)
    except subprocess.TimeoutExpired:
        terminate(proc)
        return fail("bounded MCP exchange timed out")
    child_stdout.write_bytes(stdout)
    child_stderr.write_bytes(stderr)
    if proc.returncode != 0:
        return fail(f"assay mcp wrap exited {proc.returncode}")

    try:
        responses = [json.loads(line) for line in stdout.splitlines()]
        decisions = [json.loads(line) for line in decision_log.read_bytes().splitlines()]
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        return fail(f"captured output is unreadable: {error}")
    if [item.get("id") for item in responses] != [1, 2, 3]:
        return fail("expected one response for initialize, tools/list, and tools/call")
    if len(decisions) != 1:
        return fail("expected exactly one tool decision")
    decision = decisions[0]
    if decision.get("type") != "assay.tool.decision" or decision.get("data", {}).get("tool") != "read_file":
        return fail("decision artifact identity drifted")
    if decision.get("data", {}).get("decision") != "allow":
        return fail("the local read_file call was not allowed")
    if not invocation_log.is_file():
        return fail("the local mock server was not invoked")

    print("assay quickstart: PASS")
    print("mcp_requests=initialize,tools/list,tools/call")
    print("decision=allow tool=read_file")
    print("decision_artifact=.assay/quickstart/decisions.ndjson")
    print("non_claim=forwarded_to_local_mock_only")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
