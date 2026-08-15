# The Gateway Pattern: Enterprise Runtime Enforcement

This guide documents the reference architecture for the "Gateway Pattern" configuration, designed for high-stakes enterprise deployments requiring strict protocol validation.

## 1. Architecture Overview

The Gateway Pattern positions Assay as a decision point in the runtime path.
The client remains responsible for enforcing the returned decision before it
invokes a target tool.

```mermaid
graph TD
    User((Operator)) -->|Initiates Action| Client[MCP Client]

    subgraph "Policy Enforcement Layer"
        Client -->|1. Tool Call (JSON-RPC)| Assay{Assay Gateway}

        Assay -->|Policy Eval < 1ms| PolicyDB[(Ruleset)]

        Assay -- "BLOCK decision" --> Client
        Assay -- "ALLOW decision" --> Client
    end

    Client -->|2. Enforced allowed action| Backend[System of Record]
    Client -->|3. Feedback Loop| User
    Assay -.->|4. Audit Trail| OTLP[Observability]

    style Assay fill:#00d97e,stroke:#333,stroke-width:2px,color:white
```

## 2. Configuration Strategy

The built-in stdio server always fails closed when tool dispatch fails or times
out. Availability must not silently become authorization.

### Failure behavior

Do not send `arguments.on_error`; it is not part of the advertised schemas and
does not control server behavior. A dispatch failure returns an MCP tool error:

**Client Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "assay_check_args",
    "arguments": {"tool": "approve_transaction", "arguments": {"amount": 500}, "policy": "finance_v1.yaml"}
  }
}
```

**System Response (on an outer dispatch failure):**
```json
{
  "content": [{
    "type": "text",
    "text": "{\"allowed\":false,\"error\":{\"code\":\"E_INTERNAL\",\"message\":\"Tool execution failed\"}}"
  }],
  "isError": true
}
```

For availability, use bounded retry for retryable transport failures,
supervise and restart the local server, run redundant instances where the host
supports that safely, and alert on `E_TIMEOUT` and `E_INTERNAL`. Never forward
the protected target action merely because the policy decision is unavailable.

## 3. Telemetry & Accounting

Assay emits structured logs for both Operational Monitoring and Usage Accounting.

### Metered Usage Event
Ingest these logs to calculate governance usage volume.

```json
{
  "target": "assay_billing",
  "event": "assay.usage.metered",
  "usage_type": "policy_check",
  "count": 1
}
```

### Failure alert

Trigger an alert from the structured completion event and its reason code. The
server does not emit caller values or raw internal errors in this path.

```json
{
  "event": "tool_call_done",
  "outcome": "app_error",
  "allowed": false,
  "code": "E_INTERNAL"
}
```
