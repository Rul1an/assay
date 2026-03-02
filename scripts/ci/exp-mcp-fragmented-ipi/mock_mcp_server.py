#!/usr/bin/env python3
import hashlib
import json
import os
import sys
from pathlib import Path

FIXTURE_ROOT = Path(os.environ["EXP_FIXTURE_ROOT"])
TOOL_LOG = Path(os.environ["EXP_TOOL_LOG"])


def log_tool(entry):
    TOOL_LOG.parent.mkdir(parents=True, exist_ok=True)
    with TOOL_LOG.open("a", encoding="utf-8") as handle:
        handle.write(json.dumps(entry) + "\n")


def result(msg_id, payload, is_error=False):
    body = {"content": [{"type": "text", "text": json.dumps(payload)}]}
    if is_error:
        body["isError"] = True
    return {"jsonrpc": "2.0", "id": msg_id, "result": body}


def initialize(msg_id):
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "protocolVersion": "2024-11-05",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fragmented-ipi-mock", "version": "1.0"},
        },
    }


def tools_list(msg_id):
    return {
        "jsonrpc": "2.0",
        "id": msg_id,
        "result": {
            "tools": [
                {
                    "name": "read_document",
                    "description": "Read a deterministic fixture document",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"path": {"type": "string"}},
                        "required": ["path"],
                    },
                },
                {
                    "name": "web_search",
                    "description": "Network-like sink tool used for experiment exfil tests",
                    "inputSchema": {
                        "type": "object",
                        "properties": {"query": {"type": "string"}},
                        "required": ["query"],
                    },
                },
            ]
        },
    }


def safe_fixture_path(rel_path: str) -> Path:
    candidate = (FIXTURE_ROOT / rel_path).resolve()
    if FIXTURE_ROOT.resolve() not in candidate.parents and candidate != FIXTURE_ROOT.resolve():
        raise ValueError("path traversal outside fixture root")
    return candidate


def handle_tool_call(msg_id, name, arguments):
    if name == "read_document":
        rel_path = arguments.get("path", "")
        path = safe_fixture_path(rel_path)
        content = path.read_text(encoding="utf-8")
        log_tool({
            "tool": "read_document",
            "path": rel_path,
            "sha256": hashlib.sha256(content.encode("utf-8")).hexdigest(),
            "bytes": len(content.encode("utf-8")),
        })
        return result(msg_id, {"path": rel_path, "content": content})

    if name == "web_search":
        query = arguments.get("query", "")
        log_tool({
            "tool": "web_search",
            "query": query,
            "sha256": hashlib.sha256(query.encode("utf-8")).hexdigest(),
            "bytes": len(query.encode("utf-8")),
        })
        return result(msg_id, {"query": query, "results": ["stubbed search result"]})

    return result(msg_id, {"error": {"code": "E_UNKNOWN_TOOL", "message": name}}, is_error=True)


for raw in sys.stdin:
    raw = raw.strip()
    if not raw:
        continue
    try:
        req = json.loads(raw)
        method = req.get("method")
        msg_id = req.get("id")
        if method == "initialize":
            resp = initialize(msg_id)
        elif method == "notifications/initialized":
            continue
        elif method == "tools/list":
            resp = tools_list(msg_id)
        elif method == "tools/call":
            params = req.get("params", {})
            resp = handle_tool_call(msg_id, params.get("name", ""), params.get("arguments", {}))
        else:
            resp = {"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32601, "message": "Method not found"}}
        sys.stdout.write(json.dumps(resp) + "\n")
        sys.stdout.flush()
    except Exception as exc:
        sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": None, "error": {"code": -32000, "message": str(exc)}}) + "\n")
        sys.stdout.flush()
