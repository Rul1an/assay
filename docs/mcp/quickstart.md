# MCP Quick Start

Add a policy gate to your MCP server in under 5 minutes.

## Prerequisites

- Assay CLI: `cargo install assay-cli`
- An MCP server (any stdio-based server works)

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
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

## Step 2: Write a Policy

A policy is a YAML file that says which tools are allowed and which are denied:

```yaml
# policy.yaml
version: "1.0"
name: "my-policy"
allow: ["read_file", "list_dir"]
deny: ["exec", "shell", "write_file"]
constraints:
  - tool: "read_file"
    params:
      path:
        matches: "^/app/.*"
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

## Next Steps

- [CI Integration Guide](../guides/github-action.md)
- [Evidence Store Setup](../guides/evidence-store-aws-s3.md)
- [Full Example](../../examples/mcp-quickstart/)
- [Architecture](../architecture/index.md)
