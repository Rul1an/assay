#!/usr/bin/env python3
"""Controlled JSON object-member oracles for authorize/forward tests.

Modes are abstract test doubles (first-member, last-member, strict-reject).
They are not a named-upstream bypass recipe.
"""

from __future__ import annotations

import json
import os
import sys


def first_member(pairs):
    out = {}
    for key, value in pairs:
        if key not in out:
            out[key] = value
    return out


def last_member(pairs):
    return dict(pairs)


def strict_reject(pairs):
    out = {}
    for key, value in pairs:
        if key in out:
            raise ValueError("duplicate member")
        out[key] = value
    return out


HOOKS = {
    "first": first_member,
    "last": last_member,
    "reject": strict_reject,
}


def interpret(line: str, mode: str) -> dict:
    hook = HOOKS[mode]
    try:
        msg = json.loads(line, object_pairs_hook=hook)
    except (json.JSONDecodeError, ValueError):
        return {"accepted": False}
    params = msg.get("params") if isinstance(msg, dict) else None
    if not isinstance(params, dict):
        params = {}
    arguments = params.get("arguments")
    if not isinstance(arguments, dict):
        arguments = {}
    return {
        "accepted": True,
        "method": msg.get("method") if isinstance(msg, dict) else None,
        "name": params.get("name"),
        "arguments": arguments,
        "id": msg.get("id") if isinstance(msg, dict) else None,
    }


def _append(path: str | None, text: str) -> None:
    if not path:
        return
    with open(path, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")
        handle.flush()


def _send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def serve(mode: str) -> None:
    raw_log = os.environ.get("ORACLE_RAW_LOG")
    interpret_log = os.environ.get("ORACLE_INTERPRET_LOG")
    while True:
        raw = sys.stdin.readline()
        if not raw:
            break
        line = raw.strip()
        if not line:
            continue
        _append(raw_log, line)
        seen = interpret(line, mode)
        _append(interpret_log, json.dumps(seen, separators=(",", ":")))
        if not seen.get("accepted"):
            continue
        mid = seen.get("id")
        method = seen.get("method")
        if method == "initialize":
            _send(
                {
                    "jsonrpc": "2.0",
                    "id": mid,
                    "result": {
                        "protocolVersion": "2024-11-05",
                        "capabilities": {"tools": {}},
                        "serverInfo": {"name": "member-oracle", "version": "0"},
                    },
                }
            )
        elif method == "ping":
            _send({"jsonrpc": "2.0", "id": mid, "result": {}})
        elif method == "tools/list":
            _send(
                {
                    "jsonrpc": "2.0",
                    "id": mid,
                    "result": {
                        "tools": [
                            {
                                "name": "echo",
                                "description": "echo",
                                "inputSchema": {"type": "object"},
                            }
                        ]
                    },
                }
            )
        elif method == "tools/call":
            _send(
                {
                    "jsonrpc": "2.0",
                    "id": mid,
                    "result": {
                        "content": [{"type": "text", "text": "oracle-ok"}],
                        "isError": False,
                    },
                }
            )


def main() -> None:
    if len(sys.argv) < 2:
        sys.stderr.write("usage: member_oracles.py interpret|serve [first|last|reject]\n")
        sys.exit(2)
    action = sys.argv[1]
    mode = sys.argv[2] if len(sys.argv) > 2 else os.environ.get("ORACLE_MODE", "last")
    if mode not in HOOKS:
        sys.stderr.write(f"unknown mode {mode}\n")
        sys.exit(2)
    if action == "interpret":
        line = sys.stdin.read().strip()
        sys.stdout.write(json.dumps(interpret(line, mode), separators=(",", ":")) + "\n")
        return
    if action == "serve":
        serve(mode)
        return
    sys.stderr.write(f"unknown action {action}\n")
    sys.exit(2)


if __name__ == "__main__":
    main()
