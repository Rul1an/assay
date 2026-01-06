#!/usr/bin/env python3
import sys
import json

# Minimal JSON-RPC Echo Server
# Responds to 'ping' -> 'pong'
# Logs everything to stderr

def log(msg):
    sys.stderr.write(f"[echo-server] {msg}\n")
    sys.stderr.flush()

log("Starting up...")

while True:
    try:
        line = sys.stdin.readline()
        if not line:
            break

        log(f"Received: {line.strip()}")

        req = json.loads(line)
        method = req.get("method")
        msg_id = req.get("id")

        response = None

        if method == "ping":
            response = {"jsonrpc": "2.0", "id": msg_id, "result": "pong"}
        elif method == "tools/call":
             response = {"jsonrpc": "2.0", "id": msg_id, "result": {"content": [{"type": "text", "text": "Called tool"}]}}
        else:
            # Echo unknown
             response = {"jsonrpc": "2.0", "id": msg_id, "error": {"code": -32601, "message": "Method not found"}}


        if response:
            log(f"Sending: {json.dumps(response)}")
            print(json.dumps(response))
            sys.stdout.flush()

    except Exception as e:
        log(f"Error: {e}")
        break
