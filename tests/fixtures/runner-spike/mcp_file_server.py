#!/usr/bin/env python3
import json
import sys
from pathlib import Path


def respond(message):
    print(json.dumps(message, separators=(",", ":")), flush=True)


for line in sys.stdin:
    request = json.loads(line)
    request_id = request.get("id")
    params = request.get("params") or {}
    arguments = params.get("arguments") or {}

    if request.get("method") != "tools/call" or params.get("name") != "read_file":
        respond(
            {
                "jsonrpc": "2.0",
                "id": request_id,
                "error": {"code": -32601, "message": "Method not found"},
            }
        )
        continue

    path = Path(arguments["path"])
    text = path.read_text(encoding="utf-8")
    respond(
        {
            "jsonrpc": "2.0",
            "id": request_id,
            "result": {"content": [{"type": "text", "text": text}]},
        }
    )
