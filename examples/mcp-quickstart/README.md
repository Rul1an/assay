# MCP Quickstart

See Assay block an unsafe tool call in under 2 minutes.

## What this does

Wraps a filesystem MCP server with Assay policy enforcement. The included policy
allows `read_file` and `list_dir` within `/tmp/assay-demo/` but denies everything
else. You'll see a clear ALLOW/DENY for every tool call.

## Prerequisites

- [Assay CLI](https://crates.io/crates/assay-cli): `cargo install assay-cli`
- Node.js (for the MCP filesystem server): `npm install -g @modelcontextprotocol/server-filesystem`

## Run it

```bash
# 1. Set up a demo workspace
mkdir -p /tmp/assay-demo
echo "Hello from Assay" > /tmp/assay-demo/safe.txt

# 2. Wrap the MCP server with Assay policy
assay mcp wrap \
  --policy examples/mcp-quickstart/policy.yaml \
  -- npx @modelcontextprotocol/server-filesystem /tmp/assay-demo
```

You'll see decisions for every tool call:

```
✅ ALLOW  read_file   path=/tmp/assay-demo/safe.txt   reason=policy_allow
❌ DENY   read_file   path=/tmp/outside-demo.txt       reason=path_constraint_violation
❌ DENY   exec        cmd=ls                           reason=tool_denied
```

## What's in the policy

```yaml
# policy.yaml - minimal MCP guardrail
version: "2.0"
name: "mcp-quickstart"

tools:
  allow:
    - "read_file"
    - "list_dir"
  deny:
    - "exec"
    - "shell"
    - "write_file"

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/assay-demo/.*"
        minLength: 1
    required: ["path"]

  list_dir:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/assay-demo/.*"
        minLength: 1
    required: ["path"]
```

## Next steps

- **Export evidence**: `assay evidence export --profile profile.yaml --out evidence.tar.gz`
- **Add to CI**: copy the [GitHub Action snippet](../../README.md#gate-your-ci) to your workflow
- **Generate policy from behavior**: `assay generate --from-trace trace.jsonl`
