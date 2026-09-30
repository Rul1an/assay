# MCP Quick Start

Add a policy gate to a stdio MCP server on macOS or Linux.

The wrap steps below are Unix. We ship an `x86_64-pc-windows-msvc` archive; this
page does not give a Windows walkthrough because the example policy requires
paths matching `^/tmp/assay-demo/.*`. A Windows path kept under that same
policy is denied by the policy, not by Unix path syntax.

## Prerequisites

- Assay CLI: install from a verified channel documented in the [installation guide](../getting-started/installation.md)
- Working directory: run from a source checkout or an extracted published release archive (which contains `examples/mcp-quickstart/`)
- An MCP server using the supported stdio JSON-RPC interface; verify compatibility with your server and protocol version
- An existing MCP authorization policy, such as the example in Step 2, saved as `policy.yaml` before running Step 1
- For the filesystem example: Node.js and `npx`, with permission to download and run the named npm package

## Add Assay to Cursor, Claude Desktop, Windsurf, or Zed

### Cursor

Assay has a built-in helper for Cursor:

```bash
assay mcp config-path cursor
```

That command prints the detected config location plus a ready-to-paste `mcpServers` entry.

### Claude Desktop

Assay has a built-in helper for Claude Desktop:

```bash
assay mcp config-path claude
```

That command prints the detected `claude_desktop_config.json` location plus a ready-to-paste `mcpServers` entry.

### Windsurf

The MCP configuration location depends on the installed host version and selected agent.
Open the actual MCP configuration through the host's settings or MCP panel, using the
[current vendor instructions](https://docs.windsurf.com/windsurf/cascade/mcp) for that version.
The vendor page distinguishes legacy Cascade from Devin Local; do not assume they share a
configuration file. For a host configuration using `mcpServers`, add the stdio entry below.
The host must be able to find `assay` and `npx` on its PATH; otherwise use their absolute paths.
Set the policy and filesystem directory paths to existing locations on your machine.

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

Assay only auto-detects Cursor and Claude Desktop (`assay mcp config-path claude`) today. The wrapped command can be configured in clients that support a stdio subprocess. Host configuration, executable paths and protocol compatibility still need checking in that client. Keep Claude Code and Codex static MCP configuration separate; see the [editor MCP recipe](../guides/editor-mcp-recipe.md).

## Step 1: Wrap Your Server

```bash
assay mcp wrap --policy policy.yaml -- your-mcp-server
```

Tool calls routed through this wrapper are checked before forwarding to its child server.
In normal enforcement mode, blocked calls are not forwarded. This does not cover calls made outside the wrapper or the explicit `--dry-run` mode.

### Try with the filesystem server

From the root of your source checkout or extracted release archive (which contains `examples/mcp-quickstart/policy.yaml`):

```bash
mkdir -p /tmp/assay-demo && echo "safe content" > /tmp/assay-demo/safe.txt

assay mcp wrap --policy examples/mcp-quickstart/policy.yaml --verbose \
  -- npx -y @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

Decision lines print on stderr, and only with `--verbose`. Captured stderr from
that command on macOS arm64 (assay-cli 6.5.0) after `initialize`, `tools/list`,
and one allowed `read_file` of `/tmp/assay-demo/safe.txt`:

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

For MCP authorization, use the policy example above and the [Policy Files](../reference/config/policies.md) reference. The following command generates a runtime-observation policy (`files`, `network`, and `processes`); it is not an MCP authorization policy:

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
      - uses: actions/checkout@fbc6f3992d24b796d5a048ff273f7fcc4a7b6c09 # v5.1.0
      - uses: Rul1an/assay-action@v3
```

Or run manually:

```bash
assay ci --config eval.yaml --trace-file traces/golden.jsonl
```

## Step 4: Export Evidence (Optional)

Export a bundle from an existing `profile.yaml` and the inputs that profile names. Basic wrapping does not automatically record a bundle or connect its decisions to this export; configure the required recording and profile inputs first:

```bash
assay evidence export --profile profile.yaml --out evidence.tar.gz
assay evidence verify evidence.tar.gz
```

Check the bundle against the shipped EU AI Act baseline pack. Findings and warnings describe those checks; a successful lint command is not a compliance determination:

```bash
assay evidence lint --pack eu-ai-act-baseline evidence.tar.gz
```

## Step 5: Enable Decision Logging (Optional)

To record policy-decision summaries and structured decision events for traffic observed by this wrapper:

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
| `audit.ndjson` | Tool-call policy decision summaries |
| `decisions.ndjson` | Structured `assay.tool.decision` CloudEvents with event identity and reason codes |

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
