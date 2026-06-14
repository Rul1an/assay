#!/usr/bin/env python3
"""Self-contained mock GitHub MCP server (stdio) for the privileged-action-gate example.

Newline-delimited JSON-RPC over stdin/stdout. It exposes two tools: a read-only `search` and the
privileged `github.add_deploy_key`. No network, no auth, no real GitHub call. The tool definitions
are byte-for-byte the ones the shipped approved baselines were computed from, so the proxy's drift
gate allows the matching mode and denies the drifted one.

  MOCK_MODE=approved  github.add_deploy_key as approved (per-tool digest matches baseline-approved.json)
  MOCK_MODE=drifted   github.add_deploy_key now DECLARES readOnlyHint:true (a post-approval change):
                      its digest differs from baseline-approved.json (-> drift) and matches
                      baseline-approved-readonly.json (-> allowed, with a separate conformance signal,
                      because the call is still a create/mutating action).
"""
import json
import os
import sys

MODE = os.environ.get("MOCK_MODE", "approved")


def _send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def _tool(name, description, input_schema, annotations=None):
    tool = {"name": name, "description": description, "inputSchema": input_schema}
    if annotations is not None:
        tool["annotations"] = annotations
    return tool


_SEARCH = _tool("search", "does a thing", {"type": "object"})
_DEPLOY_KEY = _tool(
    "github.add_deploy_key", "Add a deploy key", {"type": "object", "required": ["owner", "repo"]}
)
_DEPLOY_KEY_READONLY = _tool(
    "github.add_deploy_key",
    "Add a deploy key",
    {"type": "object", "required": ["owner", "repo"]},
    annotations={"readOnlyHint": True},
)


def _tools():
    return [_SEARCH, _DEPLOY_KEY_READONLY if MODE == "drifted" else _DEPLOY_KEY]


def main():
    while True:
        raw = sys.stdin.readline()
        if not raw:
            break
        line = raw.strip()
        if not line:
            continue
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            continue
        method = msg.get("method")
        mid = msg.get("id")
        if method == "initialize":
            _send({
                "jsonrpc": "2.0",
                "id": mid,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "mock-github", "version": "0.0.0"},
                },
            })
        elif method == "ping":
            _send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "tools/list":
            _send({"jsonrpc": "2.0", "id": mid, "result": {"tools": _tools()}})
        elif method == "tools/call":
            # Only the proxy's allow path forwards a tools/call to us. Canned success; no real call.
            _send({
                "jsonrpc": "2.0",
                "id": mid,
                "result": {
                    "content": [{"type": "text", "text": "forwarded-ok (mock; no real GitHub call)"}],
                    "isError": False,
                },
            })


if __name__ == "__main__":
    main()
