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
assay import --format inspector session.json --out-trace traces/session.jsonl
```

Output:
```
Imported 12 tool calls from session.json
Discovered 3 unique tools: get_customer, update_customer, send_email

Created:
  traces/session.jsonl

Next steps:
  1. Run: assay run --config eval.yaml --trace-file traces/session.jsonl
  2. Optional: scaffold policy/config with assay init --from-trace traces/session.jsonl
```

---

## Step 3: Review the Generated Config

```yaml
# eval.yaml (auto-generated)
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
assay run --config eval.yaml
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
# eval.yaml (add this test)
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
          config: eval.yaml
```

---

## Complete Example

Here's a full `eval.yaml` for a customer service agent:

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

### "Unknown format: inspector"

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

## Step 8: Enable Mandate Logging (Optional)

For audit compliance and user authorization tracking, enable CloudEvents logging:

```bash
assay mcp wrap \
  --policy assay.yaml \
  --audit-log audit.ndjson \
  --decision-log decisions.ndjson \
  --event-source "assay://myorg/myapp" \
  -- your-mcp-server
```

| Log | Purpose | Events |
|-----|---------|--------|
| `audit.ndjson` | Mandate lifecycle (audit trail) | `mandate.used`, `mandate.revoked` |
| `decisions.ndjson` | Tool decisions (high volume) | `tool.decision` (allow/deny) |

**Note:** `--event-source` is required when any logging is enabled. Use an absolute URI like `assay://org/app`.

### Audit Log Output

Each mandate consumption produces a CloudEvents record:

```json
{
  "specversion": "1.0",
  "type": "assay.mandate.used.v1",
  "id": "sha256:deterministic_use_id",
  "source": "assay://myorg/myapp",
  "data": {
    "mandate_id": "sha256:...",
    "tool_call_id": "tc_123",
    "use_count": 1
  }
}
```

The `id` field equals `use_id` (content-addressed), enabling deduplication on retries.

---

## Next Steps

- [Sequence Rules DSL](../config/sequences.md) — Advanced ordering constraints
- [Assay MCP Server](server.md) — Runtime validation for agents
- [CI Integration](../getting-started/ci-integration.md) — GitHub Actions, GitLab, Azure
- [Mandates Concept](../concepts/mandates.md) — User authorization for AI agents

---

## Time to First Eval: Under 10 Minutes

| Step | Time |
|------|------|
| Export from MCP Inspector | 1 min |
| `assay import --out-trace traces/session.jsonl` | 10 sec |
| `assay init --from-trace traces/session.jsonl` (optional) | 2 min |
| `assay run --config eval.yaml --trace-file traces/session.jsonl` | 3 sec |
| Add to CI | 5 min |
| **Total** | **~8 min** |

That's it. Your MCP agent now has deterministic regression tests.
