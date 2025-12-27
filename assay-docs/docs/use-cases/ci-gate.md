# CI Regression Gate

Catch breaking changes before they hit production.

---

## The Problem

Traditional AI agent tests are:

- **Slow:** 30 seconds to 3 minutes per test (LLM API calls)
- **Expensive:** $0.10-$1.00 per test run
- **Flaky:** 5-20% random failure rate (network, model variance)

This leads to:
- Developers ignoring test failures ("it's probably flaky")
- PRs merging without proper validation
- Bugs reaching production

---

## The Solution

Assay's CI gate provides:

- **3ms tests** — Replay traces, don't call APIs
- **$0 cost** — No API charges
- **0% flakiness** — Deterministic replay

---

## Setup

### 1. Record a Golden Trace

```bash
# Export from MCP Inspector (or your agent framework)
assay import --format mcp-inspector session.json --init
```

This creates:
- `traces/session.jsonl` — Your baseline behavior
- `mcp-eval.yaml` — Test configuration
- `policies/default.yaml` — Validation rules

### 2. Add to CI

```yaml
# .github/workflows/agent-tests.yml
name: Agent Quality Gate

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      
      - name: Install Assay
        run: cargo install assay
      
      - name: Run Tests
        run: |
          assay run \
            --config mcp-eval.yaml \
            --trace-file traces/golden.jsonl \
            --strict \
            --output sarif \
            --db :memory:
      
      - name: Upload Results
        uses: github/codeql-action/upload-sarif@v2
        if: always()
        with:
          sarif_file: .assay/reports/results.sarif
```

### 3. Configure Policies

```yaml
# mcp-eval.yaml
version: "1"
suite: agent-regression

tests:
  # Validate all tool arguments
  - id: args_valid
    metric: args_valid
    policy: policies/business-rules.yaml

  # Enforce required sequences
  - id: auth_flow
    metric: sequence_valid
    rules:
      - type: require
        tool: authenticate
      - type: before
        first: authenticate
        then: [get_data, update_data]

  # Block dangerous tools
  - id: safety
    metric: tool_blocklist
    blocklist:
      - delete_*
      - admin_*
      - debug_*

output:
  format: [sarif, junit]
  directory: .assay/reports
```

---

## Results

### Before Assay

```
PR opened → Run tests → 4 minutes → Random failure → Retry → 😤
```

### After Assay

```
PR opened → Run tests → 50ms → Deterministic result → ✅ or ❌
```

### Metrics

| Metric | Before | After |
|--------|--------|-------|
| Test duration | 3-5 min | **50ms** |
| Cost per PR | $2-5 | **$0** |
| Flake rate | 10-20% | **0%** |
| Developer trust | Low | **High** |

---

## What Gets Caught

### Argument Violations

```
❌ PR Check Failed: args_valid

   Tool: apply_discount
   Argument: percent = 50
   Violation: Value exceeds maximum (max: 30)
   
   File: prompts/discount-handler.yaml:15
```

### Sequence Violations

```
❌ PR Check Failed: sequence_valid

   Rule: auth_before_data
   Expected: authenticate before get_customer
   Actual: get_customer called without prior authenticate
   
   File: agents/customer-service.py:42
```

### Blocklist Violations

```
❌ PR Check Failed: tool_blocklist

   Blocked tool called: admin_delete
   This tool is not allowed in production agents.
   
   File: agents/admin-handler.py:88
```

---

## GitHub Integration

### SARIF Annotations

SARIF output creates inline annotations on your PR:

```
⚠️ agents/customer-service.py:42
   args_valid: percent=50 exceeds maximum (max: 30)
```

### Status Checks

The job appears as a required check:

```
✅ All checks have passed
   ✅ Agent Quality Gate (3s)
```

---

## Best Practices

### 1. Run on Every PR

```yaml
on:
  pull_request:
    branches: [main]
```

### 2. Block Merges on Failure

In GitHub: **Settings → Branches → Branch protection rules**
- ✅ Require status checks to pass
- ✅ Require "Agent Quality Gate" to pass

### 3. Keep Tests Fast

```yaml
# Use in-memory database
--db :memory:

# Skip caching in CI
--no-cache
```

### 4. Separate Fast and Slow Tests

```yaml
jobs:
  fast-tests:
    # Assay (milliseconds, free)
    - uses: Rul1an/assay-action@v1
  
  slow-tests:
    needs: fast-tests  # Only if fast tests pass
    # Real LLM tests (minutes, paid)
    - run: pytest tests/integration
```

---

## Troubleshooting

### Tests Pass Locally, Fail in CI

Check for environment differences:
- Same Assay version?
- Same trace file (check git)?
- Same policy files?

```bash
# Verify versions match
assay --version
```

### False Positives

If tests fail incorrectly:

1. **Check the violation** — Is it a real issue or policy misconfiguration?
2. **Update policy** — Loosen constraints if too strict
3. **Update trace** — Re-record if agent behavior changed intentionally

### Slow CI Jobs

If jobs take too long:

```bash
# Use in-memory mode
assay run --db :memory:

# Skip large traces
--trace-file traces/focused-test.jsonl  # Not the 1000-call log
```

---

## See Also

- [CI Integration](../getting-started/ci-integration.md)
- [assay run](../cli/run.md)
- [Policies](../concepts/policies.md)
