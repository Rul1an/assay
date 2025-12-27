# Trace-Driven Debugging

Reproduce and diagnose production failures using recorded traces.

---

## The Problem

When an AI agent fails in production:

- **Logs are incomplete** — Missing context, truncated output
- **Reproduction is hard** — "It worked when I tried it"
- **LLM is non-deterministic** — Can't recreate the exact failure
- **Time pressure** — Users waiting, SLA ticking

---

## The Solution

Assay enables **deterministic replay** of production incidents:

1. **Capture** — Record the failing session
2. **Import** — Convert to Assay trace format
3. **Replay** — Step through exactly what happened
4. **Fix** — Update policy or agent, verify fix works

---

## Workflow

### 1. Get the Incident Trace

When a user reports an issue, ask for their session log:

```bash
# From MCP Inspector export
assay import --format mcp-inspector user_session.json

# Output:
# Imported 23 tool calls from user_session.json
# Created: traces/incident-2025-12-27.jsonl
```

### 2. Reproduce the Failure

```bash
assay run --config mcp-eval.yaml --trace-file traces/incident-2025-12-27.jsonl

# Output:
# ❌ FAIL: args_valid
#    Tool: apply_discount (call #15)
#    Violation: percent=75 exceeds max(30)
```

Now you know:
- **Which tool** failed
- **What argument** was wrong
- **Exactly when** in the session it happened

### 3. Inspect in Detail

```bash
assay replay --trace traces/incident-2025-12-27.jsonl --start 13 --step

# Output:
# [13/23] get_order(id="ord_456")
#         → {"total": 150.00, "items": [...]}
#
# Press Enter to continue...
#
# [14/23] calculate_discount(total=150)
#         → {"suggested_percent": 75}
#
# [15/23] apply_discount(percent=75, order_id="ord_456")
#         → ERROR: Validation failed
```

**Root cause found:** The `calculate_discount` tool suggested 75%, but the business rule caps at 30%.

### 4. Fix and Verify

**Option A: Fix the agent** — Cap the discount before applying:

```python
suggested = calculate_discount(total)
capped = min(suggested["suggested_percent"], 30)
apply_discount(percent=capped, ...)
```

**Option B: Fix the policy** — If 75% is actually valid:

```yaml
# policies/discounts.yaml
tools:
  apply_discount:
    arguments:
      percent:
        max: 75  # Updated from 30
```

**Verify:**

```bash
assay run --config mcp-eval.yaml --trace-file traces/incident-2025-12-27.jsonl

# Output:
# ✅ All tests passed
```

---

## Interactive Debugging

### Step-by-Step Replay

```bash
assay replay --trace traces/incident.jsonl --step
```

Commands:
- `Enter` — Next call
- `i` — Inspect current call
- `j 15` — Jump to call #15
- `q` — Quit

### Verbose Mode

```bash
assay replay --trace traces/incident.jsonl --verbose
```

Shows full arguments and results for each call.

### Policy Overlay

```bash
assay replay --trace traces/incident.jsonl --policy policies/new-rules.yaml
```

Test if updated policies would have caught the issue.

---

## Real Example: Customer Service Bot

### Incident Report

> "The bot promised a 75% discount but then said it couldn't apply it. The customer is upset."

### Investigation

```bash
# Import the session
assay import --format mcp-inspector support-case-4521.json

# Find the problem
assay run --config mcp-eval.yaml --trace-file traces/support-case-4521.jsonl --verbose
```

Output:
```
[14] calculate_discount
     Input: {"customer_tier": "platinum", "order_total": 500}
     Output: {"percent": 75, "reason": "Platinum member 3x points"}

[15] apply_discount  ← FAILURE
     Input: {"percent": 75}
     Policy violation: percent exceeds max(30)
```

### Root Cause

The `calculate_discount` tool returned 75% for platinum members, but `apply_discount` has a hard cap of 30% from the fraud prevention policy.

### Fix

Updated the discount calculation to respect the cap:

```python
def calculate_discount(customer_tier, order_total):
    base_discount = get_tier_discount(customer_tier)
    return min(base_discount, MAX_DISCOUNT)  # Added cap
```

### Verification

```bash
# Re-run with fix
assay run --config mcp-eval.yaml --trace-file traces/support-case-4521.jsonl

# ✅ All tests passed
```

---

## Building a Failure Library

Over time, build a collection of failure traces:

```
traces/
├── golden/
│   └── happy-path.jsonl
├── failures/
│   ├── discount-overflow.jsonl
│   ├── missing-auth.jsonl
│   ├── blocked-tool-called.jsonl
│   └── sequence-violation.jsonl
└── edge-cases/
    ├── empty-cart.jsonl
    └── unicode-input.jsonl
```

Run all as regression tests:

```bash
assay run --config mcp-eval.yaml --trace-dir traces/
```

---

## Tips

### 1. Capture Early

Set up logging to capture all sessions, not just failures:

```python
# Log all MCP sessions
session.export_to_file(f"logs/{session_id}.json")
```

### 2. Anonymize Sensitive Data

Before sharing traces:

```bash
assay anonymize --trace incident.jsonl --output safe-incident.jsonl
```

### 3. Add to Test Suite

After fixing a bug, add the trace to CI:

```bash
cp traces/incident-2025-12-27.jsonl traces/regression/discount-cap.jsonl
git add traces/regression/discount-cap.jsonl
git commit -m "Add regression test for discount cap bug"
```

### 4. Time-Box Investigation

With Assay, debugging should take minutes, not hours:

1. **5 min** — Import and run initial test
2. **10 min** — Step through replay, identify root cause
3. **15 min** — Implement and verify fix

---

## See Also

- [assay replay](../cli/replay.md)
- [Traces](../concepts/traces.md)
- [Replay Engine](../concepts/replay.md)
