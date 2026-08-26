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

# Closed set of log basenames. Env values are admitted only as these names;
# `open` uses the dict value, never the raw env string.
ALLOWED_LOG_NAMES = {
    "raw.log": "raw.log",
    "interpret.ndjson": "interpret.ndjson",
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


def allowed_log_name(name: str | None) -> str | None:
    """Admit a single allowed basename, or refuse before any open.

    Separators, `.`, `..`, and unknown names fail. A successful return is a
    value from ALLOWED_LOG_NAMES, not the raw env string.
    """
    if not name:
        return None
    if any(sep in name for sep in ("/", "\\", os.sep)):
        raise ValueError("log name must be a single path component")
    if name in {".", ".."}:
        raise ValueError("log name must be a single path component")
    admitted = ALLOWED_LOG_NAMES.get(name)
    if admitted is None:
        raise ValueError("unknown log name")
    return admitted


def _append(name: str | None, text: str) -> None:
    admitted = allowed_log_name(name)
    if not admitted:
        return
    with open(admitted, "a", encoding="utf-8") as handle:
        handle.write(text + "\n")
        handle.flush()


def _send(obj: dict) -> None:
    sys.stdout.write(json.dumps(obj) + "\n")
    sys.stdout.flush()


def serve(mode: str) -> None:
    try:
        raw_log = allowed_log_name(os.environ.get("ORACLE_RAW_LOG"))
        interpret_log = allowed_log_name(os.environ.get("ORACLE_INTERPRET_LOG"))
    except ValueError as err:
        sys.stderr.write(f"{err}\n")
        sys.exit(2)
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
        sys.stderr.write(
            "usage: member_oracles.py interpret|serve|check-log-name [arg]\n"
        )
        sys.exit(2)
    action = sys.argv[1]
    if action == "check-log-name":
        if len(sys.argv) < 3:
            sys.stderr.write("usage: member_oracles.py check-log-name NAME\n")
            sys.exit(2)
        try:
            admitted = allowed_log_name(sys.argv[2])
        except ValueError as err:
            sys.stderr.write(f"{err}\n")
            sys.exit(1)
        if admitted is None:
            sys.exit(1)
        sys.stdout.write(admitted + "\n")
        return
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
