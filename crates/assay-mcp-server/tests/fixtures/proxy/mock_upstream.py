#!/usr/bin/env python3
"""Deterministic mock upstream MCP server (stdio) for the P61b proxy-forwarding tests.

It speaks newline-delimited JSON-RPC over stdin/stdout, records every received method (and, if asked,
the raw received line) so a test can assert what did and did not reach the upstream, and answers a
fixed set of methods with canned results. It uses no network and performs no auth/header handling.

Environment:
  MOCK_UPSTREAM_LOG       append each received `method` (one per line) to this path
  MOCK_UPSTREAM_RAW_LOG   append each raw received line to this path (to prove verbatim forwarding)
  MOCK_UPSTREAM_MODE      "normal" (default) or "malformed" (emit a non-JSON line for tools/list)
"""
import json
import os
import sys

LOG = os.environ.get("MOCK_UPSTREAM_LOG")
RAW_LOG = os.environ.get("MOCK_UPSTREAM_RAW_LOG")
MODE = os.environ.get("MOCK_UPSTREAM_MODE", "normal")

# Count of cursorless tools/list starts (for the complete_then_partial mode).
_LIST_STARTS = 0


def _append(path, text):
    if path:
        with open(path, "a", encoding="utf-8") as f:
            f.write(text + "\n")
            f.flush()


def _send(obj):
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def _tool(name, description="echoes input", input_schema=None):
    return {
        "name": name,
        "description": description,
        "inputSchema": input_schema if input_schema is not None else {"type": "object"},
    }


def _result(mid, tools, next_cursor=None):
    result = {"tools": tools}
    if next_cursor is not None:
        result["nextCursor"] = next_cursor
    _send({"jsonrpc": "2.0", "id": mid, "result": result})


# The two tools whose canonical projection equals the committed P60a canonicalization example, so the
# emitted manifest_digest matches the P60a/P60b committed digest.
_P60A_TOOLS = [
    _tool("search", "does a thing", {"type": "object"}),
    _tool(
        "github.add_deploy_key",
        "Add a deploy key",
        {"type": "object", "required": ["owner", "repo"]},
    ),
]


def _handle_tools_list(mid, cursor):
    if MODE == "malformed":
        # A non-JSON line: the proxy must not relay this as a successful response.
        sys.stdout.write("THIS IS NOT JSON-RPC\n")
        sys.stdout.flush()
    elif MODE == "p60a":
        _result(mid, _P60A_TOOLS)
    elif MODE == "paginated":
        if cursor is None:
            _result(mid, [_tool("alpha")], next_cursor="c1")
        else:
            _result(mid, [_tool("beta")])  # terminal
    elif MODE == "partial":
        # Always advertises a next page; the test never follows it, so the chain stays unfinished.
        _result(mid, [_tool("alpha")], next_cursor="c1")
    elif MODE == "duplicate":
        _result(mid, [_tool("dup"), _tool("dup", "second with same name")])
    elif MODE == "complete_then_partial":
        # First cursorless start completes; a later cursorless start advertises a next page (the test
        # never follows it), so the run ends with a complete chain plus a later incomplete one.
        global _LIST_STARTS
        if cursor is None:
            _LIST_STARTS += 1
            if _LIST_STARTS == 1:
                _result(mid, [_tool("echo")])  # complete
            else:
                _result(mid, [_tool("alpha")], next_cursor="c1")  # partial start
        else:
            _result(mid, [_tool("beta")])
    else:  # normal
        _result(mid, [_tool("echo")])


def main():
    # readline() (not `for line in sys.stdin`) so lines are handled as they arrive, not after a full
    # buffer fills; the proxy invokes this script with python -u for unbuffered streams.
    while True:
        raw = sys.stdin.readline()
        if not raw:
            break
        line = raw.strip()
        if not line:
            continue
        _append(RAW_LOG, line)
        try:
            msg = json.loads(line)
        except json.JSONDecodeError:
            # The proxy should only ever forward valid JSON; record nothing actionable.
            continue
        method = msg.get("method")
        if method:
            _append(LOG, method)
        mid = msg.get("id")

        if method == "initialize":
            _send({
                "jsonrpc": "2.0",
                "id": mid,
                "result": {
                    "protocolVersion": "2024-11-05",
                    "capabilities": {"tools": {}},
                    "serverInfo": {"name": "mock-upstream", "version": "0.0.0"},
                },
            })
            if MODE == "changed":
                # An unsolicited list-changed notification (a run fact the proxy should observe).
                _send({"jsonrpc": "2.0", "method": "notifications/tools/list_changed"})
        elif method == "ping":
            _send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "tools/list":
            _handle_tools_list(mid, (msg.get("params") or {}).get("cursor"))
        elif method is not None and mid is not None:
            # Should never happen: the proxy denies non-allowlisted methods before they reach us. If a
            # request slips through, fail loudly rather than silently accept it.
            _send({
                "jsonrpc": "2.0",
                "id": mid,
                "error": {"code": -32601, "message": "method not found"},
            })
        # Notifications (no id) get logged but no response.


if __name__ == "__main__":
    main()
