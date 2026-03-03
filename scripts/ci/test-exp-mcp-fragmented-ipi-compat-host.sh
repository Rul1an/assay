#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIX_DIR="$ROOT/scripts/ci/fixtures/exp-mcp-fragmented-ipi"
AUDIT_LOG="$ROOT/target/exp-mcp-fragmented-ipi-compat-host/audit.jsonl"
mkdir -p "$(dirname "$AUDIT_LOG")"
rm -f "$AUDIT_LOG"

COMPAT_ROOT="$FIX_DIR" COMPAT_AUDIT_LOG="$AUDIT_LOG" python3 - <<'PY'
import json
import os
import subprocess
import sys
from pathlib import Path

root = Path(os.getcwd())
def run_session(second_sink: bool):
    env = dict(os.environ)
    if second_sink:
        env["EXPERIMENT_VARIANT"] = "second_sink"
    proc = subprocess.Popen(
        ["python3", str(root / "scripts/ci/exp-mcp-fragmented-ipi/compat_host/compat_host.py")],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        env=env,
    )

    def rpc(payload, expect=True):
        proc.stdin.write(json.dumps(payload) + "\n")
        proc.stdin.flush()
        if not expect:
            return None
        line = proc.stdout.readline()
        if not line:
            raise SystemExit(proc.stderr.read())
        return json.loads(line)

    def payload_text(response):
        content = response["result"]["content"][0]["text"]
        return json.loads(content)

    try:
        init = rpc({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {"name": "compat-host-test", "version": "1.0"},
            },
        })
        assert init["result"]["serverInfo"]["name"] == "fragmented-ipi-compat-host"
        rpc({"jsonrpc": "2.0", "method": "notifications/initialized", "params": {}}, expect=False)

        listed = rpc({"jsonrpc": "2.0", "id": 2, "method": "tools/list", "params": {}})
        tool_names = sorted(tool["name"] for tool in listed["result"]["tools"])
        if second_sink:
            assert tool_names == ["read_document", "web_search", "web_search_alt"], tool_names
        else:
            assert tool_names == ["read_document", "web_search"], tool_names

        read_resp = rpc({
            "jsonrpc": "2.0",
            "id": 3,
            "method": "tools/call",
            "params": {"name": "read_document", "arguments": {"path": "invoice_with_canary.txt"}},
        })
        read_payload = payload_text(read_resp)
        assert "CANARY_" in read_payload["content"]
        assert read_payload["metadata"]["path"] == "invoice_with_canary.txt"

        search_resp = rpc({
            "jsonrpc": "2.0",
            "id": 4,
            "method": "tools/call",
            "params": {"name": "web_search", "arguments": {"query": "query CANARY_9F3A9C4B17D2E8AA"}},
        })
        search_payload = payload_text(search_resp)
        assert search_payload["metadata"]["has_canary"] is True
        assert search_payload["results"][0]["title"] == "compat-host deterministic result"

        if second_sink:
            alt_resp = rpc({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "web_search_alt", "arguments": {"query": "query CANARY_9F3A9C4B17D2E8AA"}},
            })
            alt_payload = payload_text(alt_resp)
            assert alt_payload["metadata"]["has_canary"] is True
            assert alt_payload["metadata"]["sink"] == "stub_alt"
        else:
            alt_resp = rpc({
                "jsonrpc": "2.0",
                "id": 5,
                "method": "tools/call",
                "params": {"name": "web_search_alt", "arguments": {"query": "query CANARY_9F3A9C4B17D2E8AA"}},
            })
            assert alt_resp["result"]["isError"] is True

        bad_resp = rpc({
            "jsonrpc": "2.0",
            "id": 6,
            "method": "tools/call",
            "params": {"name": "read_document", "arguments": {"path": "../Cargo.toml"}},
        })
        assert "error" in bad_resp and bad_resp["error"]["code"] == -32000
    finally:
        proc.terminate()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()

run_session(second_sink=False)
run_session(second_sink=True)
PY

python3 - "$AUDIT_LOG" <<'PY'
import json
import sys
from pathlib import Path

path = Path(sys.argv[1])
entries = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]
assert any(entry["tool"] == "read_document" for entry in entries)
assert any(entry["tool"] == "web_search" and entry["has_canary"] is True for entry in entries)
assert any(entry["tool"] == "web_search_alt" and entry["has_canary"] is True for entry in entries)
for entry in entries:
    assert "content" not in entry
    assert "query" not in entry
PY
