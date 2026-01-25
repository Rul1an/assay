# assay replay

Interactive step-by-step trace replay for debugging.

---

## Synopsis

```bash
assay replay --trace <TRACE_FILE> [OPTIONS]
```

---

## Description

Replay a trace interactively, stepping through each tool call. Useful for:

- Debugging failed tests
- Understanding agent behavior
- Inspecting specific tool calls
- Comparing expected vs. actual arguments

---

## Options

| Option | Description |
|--------|-------------|
| `--trace`, `-t` | Path to trace file |
| `--step` | Enable step-by-step mode (pause after each call) |
| `--start` | Start at specific call index |
| `--policy` | Apply policy validation during replay |
| `--verbose`, `-v` | Show full argument/result details |

---

## Examples

### Basic Replay

```bash
assay replay --trace traces/golden.jsonl

# Output:
# Trace: traces/golden.jsonl
# Tool calls: 47
# Duration: 2.3s (original)
#
# [1/47] get_customer(id="cust_123")
#        → {"name": "Alice", "email": "alice@example.com"}
#
# [2/47] update_customer(id="cust_123", email="alice@new.com")
#        → {"success": true}
#
# ... (continues automatically)
```

### Step-by-Step Mode

```bash
assay replay --trace traces/golden.jsonl --step

# Output:
# [1/47] get_customer(id="cust_123")
#        → {"name": "Alice", "email": "alice@example.com"}
#
# Press Enter to continue, 'q' to quit, 'i' to inspect...
# > [Enter]
#
# [2/47] update_customer(id="cust_123", email="alice@new.com")
#        → {"success": true}
#
# > i
# === Inspection Mode ===
# Tool: update_customer
# Arguments:
#   id: "cust_123"
#   email: "alice@new.com"
# Result:
#   success: true
# Timestamp: 2025-12-27T10:00:02Z
# ========================
```

### Start at Specific Call

```bash
# Jump to call #25
assay replay --trace traces/golden.jsonl --start 25

# Output:
# Skipping to call 25...
#
# [25/47] apply_discount(percent=50, order_id="ord_456")
#         → {"success": true, "new_total": 75.00}
```

### With Policy Validation

```bash
assay replay --trace traces/golden.jsonl --policy policies/customer.yaml

# Output:
# [1/47] get_customer(id="cust_123")
#        → {"name": "Alice"}
#        ✅ args_valid: PASS
#
# [2/47] apply_discount(percent=50, order_id="ord_456")
#        → {"success": true}
#        ❌ args_valid: FAIL
#           Violation: percent=50 exceeds max(30)
```

---

## Interactive Commands

In step mode, these commands are available:

| Command | Description |
|---------|-------------|
| `Enter` | Continue to next call |
| `q` | Quit replay |
| `i` | Inspect current call in detail |
| `j <n>` | Jump to call number n |
| `s` | Toggle step mode on/off |
| `v` | Toggle verbose mode |
| `?` | Show help |

---

## Inspection Mode

Press `i` to enter inspection mode:

```
=== Inspection Mode ===
Call Index: 25
Tool: apply_discount
Timestamp: 2025-12-27T10:00:25Z

Arguments:
  percent: 50
  order_id: "ord_456"

Result:
  success: true
  new_total: 75.00
  discount_applied: 50

Preceding Calls:
  [24] get_order(id="ord_456")
  [23] verify_identity(user_id="user_789")

Following Calls:
  [26] send_confirmation(email="alice@example.com")
  [27] log_event(type="discount_applied")

Press 'b' to go back, 'n' for next, 'q' to quit inspection...
```

---

## Verbose Output

```bash
assay replay --trace traces/golden.jsonl --verbose

# Output:
# [1/47] get_customer
#   ┌─ Arguments ─────────────────────────
#   │ {
#   │   "id": "cust_123"
#   │ }
#   ├─ Result ────────────────────────────
#   │ {
#   │   "name": "Alice",
#   │   "email": "alice@example.com",
#   │   "created_at": "2024-01-15T08:00:00Z",
#   │   "orders": [
#   │     {"id": "ord_001", "total": 150.00},
#   │     {"id": "ord_002", "total": 75.50}
#   │   ]
#   │ }
#   └─ Duration: 245ms
```

---

## Debugging Workflow

### 1. Find the Problem

```bash
# Run tests to identify failure
assay run --config mcp-eval.yaml --verbose

# Output shows:
# ❌ FAIL: args_valid
#    Tool: apply_discount (call #25)
#    Violation: percent=50 exceeds max(30)
```

### 2. Replay to That Point

```bash
# Jump to the problematic call
assay replay --trace traces/golden.jsonl --start 23 --step
```

### 3. Inspect Context

```bash
# In step mode, press 'i' to inspect
# See what happened before and after
```

### 4. Fix the Issue

Either:
- Update your policy to allow the behavior
- Update your agent to comply with the policy
- Create a new "golden" trace

---

## Output Formats

### JSON Export

```bash
assay replay --trace traces/golden.jsonl --output json > replay.json
```

### Markdown Report

```bash
assay replay --trace traces/golden.jsonl --output markdown > replay.md
```

---

## See Also

- [Traces](../concepts/traces.md)
- [Replay Engine](../concepts/replay.md)
- [assay run](run.md)
