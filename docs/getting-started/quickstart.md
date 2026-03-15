# Quick Start

Add a policy gate to your MCP server in under 5 minutes.

## Install

```bash
cargo install assay-cli
```

## Option A: Wrap an MCP Server (recommended)

The fastest path to first value. Wrap any MCP server and see ALLOW/DENY on every tool call.

**1. Create a demo workspace:**

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt
```

**2. Wrap with policy:**

```bash
assay mcp wrap --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

**3. See decisions:**

```
✅ ALLOW  read_file  path=/tmp/assay-demo/safe.txt  reason=policy_allow
❌ DENY   read_file  path=/etc/passwd                reason=path_constraint_violation
❌ DENY   exec       cmd=ls                          reason=tool_denied
```

See the [MCP quickstart example](../../examples/mcp-quickstart/) for the full walkthrough.

## Option B: Run a Smoke Test

If you don't have an MCP server handy, run the built-in smoke test:

```bash
assay init --hello-trace
assay validate --config eval.yaml --trace-file traces/hello.jsonl
```

## Option C: Import from MCP Inspector

If you already have an MCP Inspector session:

```bash
assay import --format inspector session.json --out-trace traces/session.jsonl
assay validate --config eval.yaml --trace-file traces/session.jsonl
```

## Add to CI

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

## Next Steps

- [Write a Policy](../mcp/quickstart.md#step-2-write-a-policy)
- [CI Integration Guide](../guides/github-action.md)
- [Evidence and Compliance](../guides/evidence-store-aws-s3.md)
