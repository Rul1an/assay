# Self-Correcting Agents

Build agents that validate and fix their own actions using Assay's MCP server.

---

## Overview

Self-correction allows agents to:

1. **Check** — Validate arguments/sequences before execution
2. **Fix** — Apply suggested corrections automatically
3. **Execute** — Proceed with confidence

This eliminates runtime errors from invalid tool calls.

---

## Quick Start

### 1. Start the Server

```bash
assay mcp-server --policy policies/ --port 3001
```

### 2. Call from Your Agent

```python
# Before calling apply_discount(percent=50)
result = await mcp.call_tool("assay_check_args", {
    "target_tool": "apply_discount",
    "args": {"percent": 50}
})

if result["allowed"]:
    await apply_discount(percent=50)
else:
    # Use the suggested fix
    fixed = result["suggested_fix"]  # {"percent": 30}
    await apply_discount(**fixed)
```

---

## How It Works

```
┌──────────────────────────────────────────────────────────┐
│                        Agent                              │
│                                                           │
│   1. Agent plans:  "Call apply_discount(percent=50)"     │
│                           │                               │
│                           ▼                               │
│   2. Agent checks: assay_check_args(...)                 │
│                           │                               │
│                           ▼                               │
│   3. Assay responds: ❌ {allowed: false,                 │
│                          suggested_fix: {percent: 30}}   │
│                           │                               │
│                           ▼                               │
│   4. Agent self-corrects: apply_discount(percent=30)     │
│                           │                               │
│                           ▼                               │
│   5. Success! ✅                                          │
└──────────────────────────────────────────────────────────┘
```

---

## Available Tools

### assay_check_args

Validate tool arguments against policy.

**Input:**
```json
{
  "target_tool": "apply_discount",
  "args": {
    "percent": 50,
    "order_id": "ord_123"
  }
}
```

**Output (violation):**
```json
{
  "allowed": false,
  "violations": [
    {
      "field": "percent",
      "value": 50,
      "constraint": "max: 30",
      "message": "Value exceeds maximum"
    }
  ],
  "suggested_fix": {
    "percent": 30
  }
}
```

**Output (valid):**
```json
{
  "allowed": true,
  "violations": []
}
```

### assay_check_sequence

Validate if a tool call is allowed given prior calls.

**Input:**
```json
{
  "candidate_tool": "delete_customer",
  "previous_calls": ["get_customer", "log_access"]
}
```

**Output (violation):**
```json
{
  "allowed": false,
  "reason": "Rule 'verify_before_delete' requires verify_identity before delete_customer",
  "missing": ["verify_identity"],
  "suggestion": "Call verify_identity first"
}
```

### assay_policy_decide

Combined check: arguments + sequence + blocklist.

**Input:**
```json
{
  "target_tool": "process_refund",
  "args": {"amount": 500, "order_id": "ord_123"},
  "previous_calls": ["get_order", "verify_identity"]
}
```

**Output:**
```json
{
  "decision": "allow",
  "checks": {
    "args_valid": {"passed": true},
    "sequence_valid": {"passed": true},
    "blocklist": {"passed": true}
  }
}
```

---

## Integration Examples

### Claude Desktop

Add to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "assay": {
      "command": "assay",
      "args": ["mcp-server", "--policy", "/path/to/policies"]
    }
  }
}
```

### Python Agent

```python
from anthropic import Anthropic
import json

client = Anthropic()
mcp_session = MCPSession("localhost:3001")

async def safe_execute(tool_name: str, args: dict) -> dict:
    """Execute a tool with automatic self-correction."""
    
    # Check with Assay
    check = await mcp_session.call_tool("assay_check_args", {
        "target_tool": tool_name,
        "args": args
    })
    
    if check["allowed"]:
        return await execute_tool(tool_name, args)
    
    # Apply fix if available
    if "suggested_fix" in check:
        fixed_args = {**args, **check["suggested_fix"]}
        print(f"Self-corrected: {args} → {fixed_args}")
        return await execute_tool(tool_name, fixed_args)
    
    # Can't fix automatically
    raise ValueError(f"Invalid args: {check['violations']}")
```

### LangChain

```python
from langchain.tools import Tool

class AssayValidatedTool(Tool):
    def _run(self, **kwargs):
        # Check before running
        check = assay_client.check_args(self.name, kwargs)
        
        if not check["allowed"]:
            if "suggested_fix" in check:
                kwargs = {**kwargs, **check["suggested_fix"]}
            else:
                raise ValueError(check["violations"])
        
        return self._actual_run(**kwargs)
```

---

## Policies for Self-Correction

### Provide Suggested Fixes

```yaml
# policies/discounts.yaml
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
        on_violation: suggest_clamp  # Suggests clamped value
```

### Sequence Prerequisites

```yaml
# policies/security.yaml
tools:
  delete_customer:
    requires_before:
      - verify_identity
    on_missing: suggest_call  # Tells agent to call missing tool
```

---

## Best Practices

### 1. Always Check, Never Skip

```python
# ❌ Bad: Sometimes skip checking
if confident:
    await execute_tool(...)

# ✅ Good: Always check
check = await assay_check_args(...)
if check["allowed"]:
    await execute_tool(...)
```

### 2. Log Corrections

```python
if not check["allowed"]:
    logger.info("Self-correction", 
        original=args, 
        fixed=check["suggested_fix"],
        violations=check["violations"]
    )
```

### 3. Set Retry Limits

```python
MAX_RETRIES = 3

for attempt in range(MAX_RETRIES):
    check = await assay_check_args(tool, args)
    if check["allowed"]:
        break
    args = apply_fix(args, check)
else:
    raise TooManyCorrections()
```

### 4. Monitor Correction Rates

High correction rates indicate:
- Agent prompts need improvement
- Policies are too strict
- Tool schemas are unclear

---

## Debugging

### Verbose Server Logging

```bash
assay mcp-server --policy policies/ --log-level debug
```

### Test Policies Manually

```bash
# Simulate a check
curl -X POST http://localhost:3001/tools/call \
  -H "Content-Type: application/json" \
  -d '{
    "name": "assay_check_args",
    "arguments": {
      "target_tool": "apply_discount",
      "args": {"percent": 50}
    }
  }'
```

---

## See Also

- [assay mcp-server](../cli/mcp-server.md)
- [Self-Correction Use Case](../use-cases/self-correction.md)
- [Policies](../concepts/policies.md)
