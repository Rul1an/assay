"""Replay the Unix Filesystem recipe. Run from its freshly created trial directory."""

import json
import signal
import subprocess
from pathlib import Path


def require(condition, message):
    if not condition:
        raise SystemExit(message)


def timeout(*_):
    raise SystemExit("Timed out waiting for the Filesystem recipe")


def replay():
    trial = Path.cwd()
    files = trial / "files"
    require(sorted(p.name for p in files.iterdir()) == ["note.txt"], "Use a fresh recipe directory")
    require(not Path("decisions.ndjson").exists(), "Use a fresh decision log")
    server = json.loads(Path("files-mcp.json").read_text())["mcpServers"]["files"]
    proc = subprocess.Popen(
        [server["command"], *server["args"]], stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True
    )

    def send(**fields):
        proc.stdin.write(json.dumps({"jsonrpc": "2.0", **fields}) + "\n")
        proc.stdin.flush()

    def receive():
        line = proc.stdout.readline()
        require(line, "Wrapper stopped before replying")
        return json.loads(line)

    def request(number, method, params):
        send(id=number, method=method, params=params)
        while True:
            frame = receive()
            if frame.get("id") == number and ("result" in frame or "error" in frame):
                return frame

    def call(number, name, **arguments):
        return request(number, "tools/call", {"name": name, "arguments": arguments})

    signal.signal(signal.SIGALRM, timeout)
    signal.alarm(30)
    try:
        init = request(1, "initialize", {
            "protocolVersion": "2025-11-25", "capabilities": {"roots": {}},
            "clientInfo": {"name": "filesystem-replay", "version": "1"},
        })
        require("result" in init, "Initialization failed")
        send(method="notifications/initialized")
        frame = receive()
        while frame.get("method") != "roots/list":
            frame = receive()
        send(id=frame["id"], result={"roots": [{"uri": trial.as_uri()}]})
        require("result" in request(2, "tools/list", {}), "Tool discovery failed")
        before = call(3, "list_directory", path=str(files))
        write = call(4, "write_file", path=str(files / "new.txt"), content="demo")
        after = call(5, "list_directory", path=str(files))
        parent = call(6, "list_directory", path=str(trial))
        for reply in (before, after):
            result = reply["result"]
            require(not result.get("isError") and any(
                "note.txt" in item.get("text", "") for item in result["content"]
            ), "Expected note.txt in both listings")
        require("error" in write or write.get("result", {}).get("isError"), "Expected write refusal")
        result = parent["result"]
        require(result.get("isError") and any(
            "outside allowed directories" in item.get("text", "") for item in result["content"]
        ), "Expected Filesystem to refuse the parent directory")
    finally:
        signal.alarm(0)
        proc.stdin.close()
        try:
            proc.wait(timeout=5)
        except subprocess.TimeoutExpired:
            proc.kill()
            proc.wait()
            raise SystemExit("Wrapper did not stop after stdin closed")

    require(proc.returncode == 0, "Wrapper exited with an error")
    require(not (files / "new.txt").exists(), "Unexpected file after denied write")
    rows = [json.loads(line)["data"] for line in Path("decisions.ndjson").read_text().splitlines()]
    expected = [("list_directory", "allow"), ("write_file", "deny"), ("list_directory", "allow"),
                ("list_directory", "allow")]
    require([(r["tool"], r["decision"]) for r in rows] == expected, "Unexpected decision log")
    require(rows[1]["reason_code"] == "P_TOOL_NOT_ALLOWED", "Unexpected write denial reason")
    for row in rows[:3]:
        print(row["tool"], row["decision"], *([row["reason_code"]] if row["decision"] == "deny" else []))
    print("Parent listing: Assay allowed the call; Filesystem refused the path.")


if __name__ == "__main__":
    replay()
