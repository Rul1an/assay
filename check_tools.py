import subprocess
import json
import sys

# Run the server and send 'tools/list'
cmd = ["npx", "-y", "@modelcontextprotocol/server-filesystem", "/tmp"]
process = subprocess.Popen(cmd, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=sys.stderr, text=True)

request = {
    "jsonrpc": "2.0",
    "id": 1,
    "method": "tools/list"
}

out, _ = process.communicate(input=json.dumps(request) + "\n", timeout=10)
print(out)
