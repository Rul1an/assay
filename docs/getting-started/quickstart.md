# Quick Start

Add a policy gate to your MCP server in under 5 minutes on macOS or Linux.

These wrap steps are Unix. We ship an `x86_64-pc-windows-msvc` archive; this
page does not give a Windows walkthrough because the example policy requires
paths matching `^/tmp/assay-demo/.*`. A Windows path kept under that same
policy is denied by the policy, not by Unix path syntax.

## Install

```bash
cargo install assay-cli --version 6.6.0 --locked
```

For exact stdout, exits, upgrade, and rollback behavior, use the [release-pinned agent golden path](../guides/agent-golden-path.md).

## Option A: Wrap an MCP Server (recommended)

The fastest path to first value. Wrap any MCP server. Decision lines print on
stderr, and only with `--verbose`.

**1. Create a demo workspace:**

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt
```

**2. Wrap with policy:**

```bash
assay mcp wrap --policy examples/mcp-quickstart/policy.yaml --verbose \
  -- npx -y @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

**3. See decisions:**

Send a `tools/call` through the wrap. Captured stderr from that command on
macOS arm64 (assay-cli 6.5.0) after `initialize`, `tools/list`, and one allowed
`read_file` of `/tmp/assay-demo/safe.txt`:

```text
[assay] loading policy from examples/mcp-quickstart/policy.yaml
[assay] wrapping command: npx ["-y", "@modelcontextprotocol/server-filesystem", "/tmp/assay-demo"]
[assay] ALLOW read_file
Secure MCP Filesystem Server running on stdio
```

The same command, after a `read_file` of `/tmp/outside-demo.txt`:

```text
[assay] DENY read_file (reason: JSON Schema validation failed)
```

The same command, after an `exec` call:

```text
[assay] DENY exec (reason: Tool is explicitly denylisted by name)
```

Without `--verbose`, those ALLOW/DENY lines are not printed. A no-flag run
still prints the loading-policy and wrapping-command lines plus the child's
banner; a denied call still returns a JSON-RPC deny on stdout. Missing
decision lines are not a clean run.

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
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
      - uses: Rul1an/assay-action@v3
```

## Next Steps

- [Write a Policy](../mcp/quickstart.md#step-2-write-a-policy)
- [CI Integration Guide](../guides/github-action.md)
- [Evidence and Compliance](../guides/evidence-store-aws-s3.md)
