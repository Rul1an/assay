# Quick Start

Run your first Assay test in 60 seconds.

---

## Prerequisites

- Assay installed ([installation guide](installation.md))
- An MCP session log (or use our example below)

---

## Step 1: Get a Sample Session

If you don't have an MCP session yet, use our example:

```bash
# Download sample session
curl -O https://raw.githubusercontent.com/Rul1an/assay/main/examples/session.json
```

Or create your own by exporting from [MCP Inspector](https://github.com/modelcontextprotocol/inspector).

---

## Step 2: Import the Session

```bash
assay import --format mcp-inspector session.json --init
```

Output:
```
Imported 47 tool calls from session.json
Discovered 5 unique tools: apply_discount, get_customer, update_customer, verify_identity, send_email

Created:
  traces/session-2025-12-27.jsonl
  mcp-eval.yaml (default config with discovered tools)
  policies/default.yaml (template policy)

Next steps:
  1. Review policies/default.yaml and add constraints
  2. Run: assay run --config mcp-eval.yaml
```

The `--init` flag auto-generates:

| File | Purpose |
|------|---------|
| `traces/*.jsonl` | Normalized trace (your "golden" behavior) |
| `mcp-eval.yaml` | Test configuration |
| `policies/default.yaml` | Policy template to customize |

---

## Step 3: Run Tests

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
│ args_valid        │ ✅ PASS │ 47/47 calls valid       │
│ sequence_valid    │ ✅ PASS │ All sequences correct   │
│ tool_blocklist    │ ✅ PASS │ No blocked tools called │
└───────────────────┴────────┴─────────────────────────┘

Total: 3ms | 3 passed, 0 failed
Exit code: 0
```

---

## Step 4: Add a Constraint

Edit `policies/default.yaml` to add a rule:

```yaml
# policies/default.yaml
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30  # ← Add this constraint
```

Now if your agent tries to apply a 50% discount, Assay will catch it:

```bash
assay run --config mcp-eval.yaml
```

```
❌ FAIL: args_valid

   Tool: apply_discount
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
   Policy: policies/default.yaml:8

   Suggestion: Use percent <= 30
```

---

## Step 5: Add to CI

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

Every PR now gets instant, deterministic validation.

---

## What Just Happened?

1. **Import** converted your MCP session to a normalized trace
2. **Policies** defined what "correct" means (argument constraints, sequences)
3. **Run** replayed the trace and validated against policies
4. **CI** catches regressions before they hit production

No LLM calls. No network. No flakiness.

---

## Next Steps

<div class="grid cards" markdown>

-   :material-file-document:{ .lg .middle } __Your First Test__

    ---

    Write a custom policy from scratch.

    [:octicons-arrow-right-24: First test](first-test.md)

-   :material-github:{ .lg .middle } __CI Integration__

    ---

    Add Assay to GitHub Actions, GitLab, or Azure.

    [:octicons-arrow-right-24: CI guide](ci-integration.md)

-   :material-shield-check:{ .lg .middle } __Sequence Rules__

    ---

    Enforce tool call order (e.g., "verify before delete").

    [:octicons-arrow-right-24: Sequences](../config/sequences.md)

-   :material-protocol:{ .lg .middle } __MCP Deep Dive__

    ---

    Advanced MCP integration patterns.

    [:octicons-arrow-right-24: MCP guide](../mcp/index.md)

</div>

---

## Troubleshooting

### "No trace file found"

Make sure you ran `assay import` first:

```bash
assay import --format mcp-inspector session.json --init
```

### "Config version mismatch"

Run the migration command:

```bash
assay migrate --config mcp-eval.yaml
```

### "Unknown tool in policy"

The tool name in your policy must match exactly what's in the trace. Check with:

```bash
assay inspect --trace traces/session.jsonl --tools
```

---

## Video Walkthrough

*Coming soon — 60-second demo: Import → Run → CI*

<!-- Uncomment when video is ready
<video controls width="100%">
  <source src="/assets/quickstart-demo.mp4" type="video/mp4">
  Your browser does not support the video tag.
</video>
-->
