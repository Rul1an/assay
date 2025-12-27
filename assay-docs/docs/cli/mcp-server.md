# assay mcp-server

Start Assay as an MCP tool server for agent self-correction.

---

## Synopsis

```bash
assay mcp-server --policy <POLICY_DIR> [OPTIONS]
```

---

## Description

Runs Assay as a Model Context Protocol (MCP) server, exposing validation tools that agents can call at runtime. This enables:

- Agent self-correction before executing actions
- Runtime policy enforcement
- Dynamic argument validation

---

## Options

| Option | Description |
|--------|-------------|
| `--policy`, `-p` | Directory containing policy files |
| `--port` | Server port (default: 3000) |
| `--host` | Server host (default: 127.0.0.1) |
| `--log-level` | Logging verbosity: debug, info, warn, error |

---

## Examples

### Basic Usage

```bash
assay mcp-server --policy policies/

# Output:
# Assay MCP Server v0.8.0
# Listening on http://127.0.0.1:3000
# Policies loaded: 3 files
# Tools exposed: assay_check_args, assay_check_sequence, assay_policy_decide
```

### Custom Port

```bash
assay mcp-server --policy policies/ --port 3001
```

### Network Accessible

```bash
assay mcp-server --policy policies/ --host 0.0.0.0 --port 3000
```

---

## Exposed Tools

The server exposes three MCP tools:

### assay_check_args

Validate tool arguments before execution.

**Request:**
```json
{
  "tool": "assay_check_args",
  "arguments": {
    "target_tool": "apply_discount",
    "args": { "percent": 50 }
  }
}
```

**Response (violation):**
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

**Response (valid):**
```json
{
  "allowed": true,
  "violations": []
}
```

### assay_check_sequence

Validate if a tool call is allowed given the current sequence.

**Request:**
```json
{
  "tool": "assay_check_sequence",
  "arguments": {
    "candidate_tool": "delete_customer",
    "previous_calls": ["get_customer"]
  }
}
```

**Response (violation):**
```json
{
  "allowed": false,
  "reason": "Rule 'verify_before_delete' requires verify_identity before delete_customer",
  "missing": ["verify_identity"]
}
```

### assay_policy_decide

Combined check: arguments + sequence + blocklist.

**Request:**
```json
{
  "tool": "assay_policy_decide",
  "arguments": {
    "target_tool": "process_refund",
    "args": { "amount": 500, "order_id": "ord_123" },
    "previous_calls": ["get_order", "verify_identity"]
  }
}
```

**Response:**
```json
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

## Agent Integration

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

### Custom Agent

```python
import anthropic

# Agent checks before calling a tool
async def call_tool_safely(tool_name: str, args: dict):
    # First, check with Assay
    check_result = await mcp_client.call_tool(
        "assay_check_args",
        {"target_tool": tool_name, "args": args}
    )
    
    if not check_result["allowed"]:
        # Self-correct using suggested fix
        if "suggested_fix" in check_result:
            args = {**args, **check_result["suggested_fix"]}
        else:
            raise ValueError(f"Invalid args: {check_result['violations']}")
    
    # Now safe to call
    return await call_actual_tool(tool_name, args)
```

---

## Self-Correction Flow

```
┌─────────────────┐
│  Agent wants    │
│  to call tool   │
└────────┬────────┘
         │
         ▼
┌─────────────────┐     ┌─────────────────┐
│ assay_check_args│────►│    Assay MCP    │
│  (validation)   │     │     Server      │
└────────┬────────┘     └────────┬────────┘
         │                       │
         ▼                       ▼
    ┌─────────┐             ┌─────────┐
    │ allowed │             │ denied  │
    └────┬────┘             └────┬────┘
         │                       │
         ▼                       ▼
┌─────────────────┐     ┌─────────────────┐
│  Execute tool   │     │  Apply fix or   │
│    normally     │     │  ask for help   │
└─────────────────┘     └─────────────────┘
```

---

## Policies

The server loads policies from the specified directory:

```
policies/
├── customer.yaml
├── payments.yaml
└── admin.yaml
```

Policy changes are hot-reloaded (no restart needed).

### Example Policy

```yaml
# policies/customer.yaml
tools:
  apply_discount:
    arguments:
      percent:
        type: number
        min: 0
        max: 30
      order_id:
        type: string
        required: true

  delete_customer:
    requires:
      - verify_identity
    blocklist_contexts:
      - untrusted
```

---

## Logging

### Debug Mode

```bash
assay mcp-server --policy policies/ --log-level debug

# Output:
# [DEBUG] Loading policy: policies/customer.yaml
# [DEBUG] Registered tool: apply_discount (3 constraints)
# [DEBUG] Registered tool: delete_customer (1 prerequisite)
# [INFO] Server ready on http://127.0.0.1:3000
# [DEBUG] Incoming request: assay_check_args
# [DEBUG] Tool: apply_discount, Args: {"percent": 50}
# [DEBUG] Violation: percent exceeds max(30)
```

### Log to File

```bash
assay mcp-server --policy policies/ 2>&1 | tee assay-server.log
```

---

## Health Check

The server exposes a health endpoint:

```bash
curl http://127.0.0.1:3000/health

# Response:
# {"status": "healthy", "policies": 3, "uptime": "2h 15m"}
```

---

## Production Deployment

### Docker

```dockerfile
FROM rust:latest as builder
RUN cargo install assay

FROM debian:bookworm-slim
COPY --from=builder /usr/local/cargo/bin/assay /usr/local/bin/
COPY policies/ /policies/

CMD ["assay", "mcp-server", "--policy", "/policies", "--host", "0.0.0.0"]
```

### Kubernetes

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: assay-server
spec:
  replicas: 2
  template:
    spec:
      containers:
        - name: assay
          image: your-registry/assay:v0.8.0
          args: ["mcp-server", "--policy", "/policies", "--host", "0.0.0.0"]
          ports:
            - containerPort: 3000
          volumeMounts:
            - name: policies
              mountPath: /policies
      volumes:
        - name: policies
          configMap:
            name: assay-policies
```

---

## See Also

- [Self-Correction Guide](../mcp/self-correction.md)
- [MCP Integration](../mcp/index.md)
- [Policies](../concepts/policies.md)
