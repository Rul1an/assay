# Agent Self-Correction

Let agents validate their own actions before executing them.

---

## The Problem

AI agents make mistakes at runtime:

- **Invalid arguments** — Wrong types, out-of-range values
- **Sequence violations** — Skipping required steps
- **Policy breaches** — Calling forbidden tools
- **Hallucinated schemas** — Made-up parameter names

Traditional solutions:
- **Hope for the best** — Let errors happen, apologize later
- **Hardcode validation** — Brittle, not maintainable
- **Human review** — Slow, doesn't scale

---

## The Solution

Assay's MCP server lets agents **check before acting**:

```
Agent: "I want to call apply_discount(percent=50)"
Assay: "❌ percent exceeds max(30). Try percent=30."
Agent: "OK, calling apply_discount(percent=30)"
Assay: "✅ Allowed"
```

The agent self-corrects without human intervention.

---

## Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                         Agent                                │
│  ┌─────────────────┐                                        │
│  │ "I want to call │                                        │
│  │  apply_discount │                                        │
│  │  (percent=50)"  │                                        │
│  └────────┬────────┘                                        │
│           │                                                  │
│           ▼                                                  │
│  ┌─────────────────┐     ┌─────────────────┐                │
│  │ assay_check_args│────►│  Assay Server   │                │
│  └────────┬────────┘     └────────┬────────┘                │
│           │                       │                          │
│           ▼                       ▼                          │
│  ┌─────────────────┐     ┌─────────────────┐                │
│  │ ❌ Denied        │     │ suggested_fix:  │                │
│  │ percent > 30    │◄────│ {percent: 30}   │                │
│  └────────┬────────┘     └─────────────────┘                │
│           │                                                  │
│           ▼                                                  │
│  ┌─────────────────┐                                        │
│  │ Self-correct:   │                                        │
│  │ percent = 30    │                                        │
│  └────────┬────────┘                                        │
│           │                                                  │
│           ▼                                                  │
│  ┌─────────────────┐                                        │
│  │ Execute tool    │                                        │
│  │ successfully    │                                        │
│  └─────────────────┘                                        │
└─────────────────────────────────────────────────────────────┘
```

---

## Setup

### 1. Start Assay Server

```bash
assay mcp-server --policy policies/ --port 3001
```

### 2. Connect Your Agent

**Claude Desktop:**

```json
{
  "mcpServers": {
    "assay": {
      "command": "assay",
      "args": ["mcp-server", "--policy", "./policies"]
    }
  }
}
```

**Custom Agent:**

```python
# Before calling any tool
check_result = await mcp_client.call_tool(
    "assay_check_args",
    {"target_tool": tool_name, "args": args}
)

if check_result["allowed"]:
    await execute_tool(tool_name, args)
else:
    # Self-correct
    fixed_args = {**args, **check_result.get("suggested_fix", {})}
    await execute_tool(tool_name, fixed_args)
```

---

## Available Checks

### assay_check_args

Validate arguments before calling a tool.

```json
// Request
{
  "target_tool": "apply_discount",
  "args": { "percent": 50 }
}

// Response
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
  "suggested_fix": { "percent": 30 }
}
```

### assay_check_sequence

Validate if a tool is allowed given prior calls.

```json
// Request
{
  "candidate_tool": "delete_customer",
  "previous_calls": ["get_customer"]
}

// Response
{
  "allowed": false,
  "reason": "verify_identity required before delete_customer",
  "missing": ["verify_identity"]
}
```

### assay_policy_decide

Combined check (args + sequence + blocklist).

```json
// Request
{
  "target_tool": "process_refund",
  "args": { "amount": 100 },
  "previous_calls": ["get_order", "verify_identity"]
}

// Response
{
  "decision": "allow",
  "checks": {
    "args_valid": { "passed": true },
    "sequence_valid": { "passed": true },
    "blocklist": { "passed": true }
  }
}
```

---

## Self-Correction Patterns

### Pattern 1: Check-Then-Execute

```python
async def safe_tool_call(tool_name, args):
    # Check first
    result = await assay_check_args(tool_name, args)
    
    if result["allowed"]:
        return await execute_tool(tool_name, args)
    
    # Apply suggested fix
    if "suggested_fix" in result:
        fixed_args = {**args, **result["suggested_fix"]}
        return await execute_tool(tool_name, fixed_args)
    
    # Can't fix — report error
    raise ValidationError(result["violations"])
```

### Pattern 2: Retry with Feedback

```python
async def tool_with_retry(tool_name, args, max_retries=3):
    for attempt in range(max_retries):
        result = await assay_check_args(tool_name, args)
        
        if result["allowed"]:
            return await execute_tool(tool_name, args)
        
        # Ask LLM to fix based on feedback
        args = await llm_fix_args(
            tool_name, 
            args, 
            result["violations"]
        )
    
    raise MaxRetriesExceeded()
```

### Pattern 3: Pre-Flight Check

```python
async def plan_and_execute(plan: List[ToolCall]):
    # Validate entire plan first
    for call in plan:
        result = await assay_policy_decide(
            call.tool,
            call.args,
            [c.tool for c in plan[:plan.index(call)]]
        )
        if result["decision"] != "allow":
            return {"error": "Plan validation failed", "details": result}
    
    # Execute validated plan
    for call in plan:
        await execute_tool(call.tool, call.args)
```

---

## Real Example: E-commerce Agent

### Policy

```yaml
# policies/ecommerce.yaml
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
      reason:
        type: string
        required: true
```

### Agent Behavior

**Without self-correction:**
```
User: "Give me the best discount you can"
Agent: apply_discount(percent=50)
Error: Invalid argument
Agent: "Sorry, something went wrong..."
```

**With self-correction:**
```
User: "Give me the best discount you can"
Agent: [checks] assay_check_args(apply_discount, {percent: 50})
Assay: {allowed: false, suggested_fix: {percent: 30}}
Agent: apply_discount(percent=30, reason="Customer request")
Success!
Agent: "I've applied a 30% discount, the maximum available."
```

---

## Benefits

| Aspect | Without Self-Correction | With Self-Correction |
|--------|------------------------|----------------------|
| Error rate | 5-15% of tool calls | ~0% |
| User experience | Errors, apologies | Smooth execution |
| Recovery time | Retry loop with user | Instant self-fix |
| Consistency | Varies by prompt | Policy-enforced |

---

## Monitoring

### Log Corrections

```python
async def safe_tool_call(tool_name, args):
    result = await assay_check_args(tool_name, args)
    
    if not result["allowed"]:
        logger.info(
            "Self-correction applied",
            tool=tool_name,
            original=args,
            fixed=result.get("suggested_fix"),
            violations=result["violations"]
        )
    
    # ...
```

### Metrics to Track

- **Correction rate** — % of calls requiring fixes
- **Violation types** — Which constraints trigger most?
- **Fix success rate** — Do suggested fixes work?

---

## See Also

- [assay mcp-server](../cli/mcp-server.md)
- [MCP Integration](../mcp/index.md)
- [Policies](../concepts/policies.md)
