# MCP Quick Start

Add a policy gate to your MCP server in under 5 minutes.

## Prerequisites

- Assay CLI: `cargo install assay-cli`
- An MCP server (any stdio-based server works)

## Add Assay to Cursor, Windsurf, or Zed

### Cursor

Assay has a built-in helper for Cursor:

```bash
assay mcp config-path cursor
```

That command prints the detected config location plus a ready-to-paste `mcpServers` entry.

### Windsurf

Windsurf uses `mcpServers` in `~/.codeium/windsurf/mcp_config.json`.
Use the same wrapped command Assay generates for Cursor:

```json
{
  "mcpServers": {
    "filesystem-secure": {
      "command": "assay",
      "args": [
        "mcp",
        "wrap",
        "--policy",
        "/path/to/policy.yaml",
        "--",
        "npx",
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "/Users/you"
      ]
    }
  }
}
```

### Zed

Zed stores custom MCP commands under `context_servers` in the settings JSON:

```json
{
  "context_servers": {
    "filesystem-secure": {
      "command": "assay",
      "args": [
        "mcp",
        "wrap",
        "--policy",
        "/path/to/policy.yaml",
        "--",
        "npx",
        "-y",
        "@modelcontextprotocol/server-filesystem",
        "/Users/you"
      ]
    }
  }
}
```

Assay only auto-detects Cursor and Claude today, but the wrapped command itself is portable across MCP clients.

## Step 1: Wrap Your Server

```bash
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

Every tool call now passes through Assay's policy engine before reaching the server.
Blocked calls never reach the server.

### Try with the filesystem server

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt

assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

Output:

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
❌ DENY   read_file  path=/tmp/outside-demo.txt      reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

## Step 2: Write a Policy

A policy is a YAML file that says which tools are allowed and which are denied:

```yaml
# policy.yaml
version: "2.0"
name: "my-policy"

tools:
  allow: ["read_file", "list_dir"]
  deny: ["exec", "shell", "write_file"]

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/app/.*"
        minLength: 1
    required: ["path"]
```

Or generate one from what your agent actually does:

```bash
assay init --from-trace trace.jsonl
```

## Step 3: Add to CI

```yaml
# .github/workflows/assay.yml
name: Assay Gate
on: [push, pull_request]
permissions:
  contents: read
  security-events: write
jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay-action@v2
```

Or run manually:

```bash
assay ci --config eval.yaml --trace-file traces/golden.jsonl
```

## Step 4: Export Evidence (Optional)

Every decision produces auditable evidence:

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
```

Lint against compliance packs:

```bash
assay evidence lint --pack eu-ai-act-baseline evidence.tar.gz
```

## Step 5: Enable Decision Logging (Optional)

For full audit trails:

```bash
assay mcp wrap \
  --policy policy.yaml \
  --audit-log audit.ndjson \
  --decision-log decisions.ndjson \
  --event-source "assay://myorg/myapp" \
  -- your-mcp-server
```

| Log | Purpose |
|-----|---------|
| `audit.ndjson` | Mandate lifecycle events |
| `decisions.ndjson` | Tool-call ALLOW/DENY decisions |

## Step 6: Reuse Existing OTel / Langfuse Traces (Optional)

If your agent stack already emits OpenTelemetry spans, import them instead of recapturing everything:

```bash
assay trace ingest-otel \
  --input otel-export.jsonl \
  --db .eval/eval.db \
  --out-trace traces/otel.v2.jsonl
```

That gives you replayable Assay traces you can reuse in your assertions pipeline.

## Next Steps

- [Operator Proof Flow](../guides/operator-proof-flow.md)
- [CI Integration Guide](../guides/github-action.md)
- [OpenTelemetry & Langfuse](../guides/otel-langfuse.md)
- [Evidence Store Setup](../guides/evidence-store-aws-s3.md)
- [Full Example](../../examples/mcp-quickstart/)
- [Architecture](../architecture/index.md)
