# Your First Test

Write an MCP authorization policy and check one allowed and one denied call against
a local mock. The mock performs no file read or external action: this tests the
wrapper's policy decision, not whether a real provider completed an action.

## Prerequisites

- Assay CLI 6.9.0 on `PATH` and Python 3.
- A source checkout or extracted 6.9.0 CLI archive containing
  `examples/mcp-quickstart/run.py` and `mock_server.py`.
- A Unix shell and a writable directory. Start at the root of that checkout or
  archive, with no existing `assay-first-test` directory.

No model, credentials, npm package or network connection is needed for this local
exercise. Install the CLI separately using the [installation guide](installation.md).

## 1. Create a separate test directory

```bash
mkdir assay-first-test
cp examples/mcp-quickstart/run.py examples/mcp-quickstart/mock_server.py assay-first-test/
cd assay-first-test
```

## 2. Write your policy

Create `policy.yaml` with this content:

```yaml
version: "2.0"
name: "my-first-policy"
tools:
  allow: ["read_file"]
  deny: ["exec"]
schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/assay-demo/.*"
    required: ["path"]
```

This allows `read_file` only when its `path` argument matches the shown prefix,
and explicitly denies `exec`. The policy checks request arguments; it does not
resolve symlinks or prove a filesystem access occurred.

## 3. Check the allowed call

```bash
python3 run.py
```

The runner sends `initialize`, `tools/list`, and a `read_file` call naming
`/tmp/assay-demo/safe.txt`. Expect exit 0 and `assay quickstart: PASS`.
It retains the response and one allow decision under `.assay/quickstart/`.
No real file at that path is required because the child is the bundled mock.

## 4. Make the same call fail the policy

Retain the allowed result, then change the allowed path prefix:

```bash
mv .assay/quickstart .assay/allowed
python3 - <<'PY'
from pathlib import Path
p = Path("policy.yaml")
text = p.read_text()
assert text.count("^/tmp/assay-demo/.*") == 1
p.write_text(text.replace("^/tmp/assay-demo/.*", "^/different-root/.*"))
PY
python3 run.py
```

The last command is expected to exit 1: the runner requires an allowed call, and
this policy now denies it. A nonzero exit alone is not sufficient evidence of the
intended denial. Check the structured decision:

```bash
python3 - <<'PY'
import json
from pathlib import Path
records = [json.loads(line) for line in Path(".assay/quickstart/decisions.ndjson").read_text().splitlines()]
assert len(records) == 1
assert records[0]["type"] == "assay.tool.decision"
assert records[0]["data"]["tool"] == "read_file"
assert records[0]["data"]["decision"] == "deny"
assert records[0]["data"]["reason_code"] == "P_ARG_SCHEMA"
print("Expected policy denial recorded")
PY
```

The allowed artifacts remain in `.assay/allowed/`; the denied run is in
`.assay/quickstart/`. The runner refuses to overwrite an existing result directory.
This contrast checks one rule against one request. It is not coverage of every
policy rule, a real-file access test, or a compliance result.

Next, adapt the policy to your actual server using the [MCP quickstart](../mcp/quickstart.md).
