# MCP Quick Start

Import an MCP session and run your first test in 5 minutes.

---

## Prerequisites

- Assay installed ([installation guide](../getting-started/installation.md))
- An MCP session from [MCP Inspector](https://github.com/modelcontextprotocol/inspector)

---

## Step 1: Export from MCP Inspector

In MCP Inspector, run your agent session, then export:

**File → Export Session → JSON**

You'll get a file like `session.json`:

```json
{
  "messages": [
    {
      "jsonrpc": "2.0",
      "id": 1,
      "method": "tools/call",
      "params": {
        "name": "get_customer",
        "arguments": { "id": "cust_123" }
      }
    },
    {
      "jsonrpc": "2.0",
      "id": 1,
      "result": {
        "content": [{ "type": "text", "text": "{\"name\": \"Alice\"}" }]
      }
    }
  ]
}
```

---

## Step 2: Import into Assay

```bash
assay import --format mcp-inspector session.json --init
```

Output:
```
Imported 12 tool calls from session.json
Discovered 3 unique tools: get_customer, update_customer, send_email

Created:
  traces/session-2025-12-27.jsonl
  mcp-eval.yaml
  policies/default.yaml

Next steps:
  1. Review policies/default.yaml
  2. Run: assay run --config mcp-eval.yaml
```

The `--init` flag auto-generates everything you need.

---

## Step 3: Review the Generated Config

```yaml
# mcp-eval.yaml (auto-generated)
version: "1"
suite: mcp-basics

tests:
  - id: args_valid_all
    metric: args_valid
    policy: policies/default.yaml

  - id: no_blocked_tools
    metric: tool_blocklist
    blocklist: []  # Add dangerous tools here

output:
  format: [sarif, junit]
  directory: .assay/reports
```

---

## Step 4: Add Constraints

Edit `policies/default.yaml` to add validation rules:

```yaml
# policies/default.yaml
tools:
  get_customer:
    arguments:
      id:
        type: string
        pattern: "^cust_[0-9]+$"
  
  update_customer:
    arguments:
      id:
        type: string
        required: true
      email:
        type: string
        format: email
  
  send_email:
    arguments:
      to:
        type: string
        format: email
      subject:
        type: string
        maxLength: 200
```

---

## Step 5: Run Tests

```bash
assay run --config mcp-eval.yaml
```

Output:
```
Assay v0.8.0 — Zero-Flake CI for AI Agents

Suite: mcp-basics
Trace: traces/session-2025-12-27.jsonl

┌───────────────────┬────────┬─────────────────────────┐
│ Test              │ Status │ Details                 │
├───────────────────┼────────┼─────────────────────────┤
│ args_valid_all    │ ✅ PASS │ 12/12 calls valid       │
│ no_blocked_tools  │ ✅ PASS │ No blocked tools called │
└───────────────────┴────────┴─────────────────────────┘

Total: 2ms | 2 passed, 0 failed
```

---

## Step 6: Add Sequence Rules

Ensure tools are called in the correct order:

```yaml
# mcp-eval.yaml (add this test)
tests:
  # ... existing tests ...
  
  - id: read_before_write
    metric: sequence_valid
    rules:
      - type: before
        first: get_customer
        then: update_customer
```

Now if your agent updates a customer without first reading their data, the test fails.

---

## Step 7: Add to CI

```yaml
# .github/workflows/agent-tests.yml
name: Agent Quality Gate

on: [push, pull_request]

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay-action@v1
        with:
          config: mcp-eval.yaml
```

---

## Complete Example

Here's a full `mcp-eval.yaml` for a customer service agent:

```yaml
version: "1"
suite: customer-service-agent

tests:
  # Validate all tool arguments
  - id: args_valid
    metric: args_valid
    policy: policies/customer-service.yaml

  # Enforce call sequences
  - id: auth_before_access
    metric: sequence_valid
    rules:
      - type: require
        tool: authenticate_user
      - type: before
        first: authenticate_user
        then: [get_customer, update_customer, delete_customer]

  # Block dangerous tools
  - id: no_admin_tools
    metric: tool_blocklist
    blocklist:
      - admin_*
      - system_*
      - delete_database

  # Limit API calls
  - id: rate_limit
    metric: sequence_valid
    rules:
      - type: count
        tool: external_api
        max: 10

output:
  format: [sarif, junit]
  directory: .assay/reports
```

---

## Troubleshooting

### "Unknown format: mcp-inspector"

Update to the latest Assay version:

```bash
cargo install assay --force
```

### "No tool calls found"

Your session might not contain `tools/call` messages. Check the JSON:

```bash
cat session.json | jq '.messages[] | select(.method == "tools/call")'
```

### "Schema validation error"

The generated policy might not match your tool signatures. Edit `policies/default.yaml` to match your actual argument types.

---

## Next Steps

- [Sequence Rules DSL](../config/sequences.md) — Advanced ordering constraints
- [Assay MCP Server](server.md) — Runtime validation for agents
- [CI Integration](../getting-started/ci-integration.md) — GitHub Actions, GitLab, Azure

---

## Time to First Eval: Under 10 Minutes

| Step | Time |
|------|------|
| Export from MCP Inspector | 1 min |
| `assay import --init` | 10 sec |
| Review config | 2 min |
| `assay run` | 3 sec |
| Add to CI | 5 min |
| **Total** | **~8 min** |

That's it. Your MCP agent now has deterministic regression tests.
