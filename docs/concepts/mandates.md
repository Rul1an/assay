# Mandates: User Authorization for AI Agents

> **Audience:** Product managers, compliance officers, security teams, and developers new to Assay.

## What is a Mandate?

A **mandate** is cryptographic proof that a user authorized an AI agent to perform specific actions. Think of it as a digital "permission slip" that:

- **Proves authorization**: Links agent actions to explicit user consent
- **Limits scope**: Restricts what the agent can do (tools, amounts, time)
- **Enables audit**: Creates tamper-proof evidence for compliance

```mermaid
flowchart LR
    subgraph User[User]
        AUTH[Grants Permission]
    end

    subgraph Mandate[Mandate]
        SCOPE[Allowed Actions]
        LIMITS[Spending Limits]
        TIME[Valid Until]
        SIG[Digital Signature]
    end

    subgraph Agent[AI Agent]
        TOOL[Tool Call]
    end

    subgraph Evidence[Audit Trail]
        USED[Consumption Receipt]
        DEC[Decision Record]
    end

    AUTH --> Mandate
    Mandate --> |authorizes| TOOL
    TOOL --> USED
    TOOL --> DEC
```

## Why Mandates Matter

### The Problem Without Mandates

When AI agents act autonomously, critical questions arise:

| Question | Without Mandates | With Mandates |
|----------|-----------------|---------------|
| Did the user approve this purchase? | Unknown | Cryptographic proof |
| What was the spending limit? | Implicit | Explicit in mandate |
| When did authorization expire? | Never defined | `expires_at` timestamp |
| Was the mandate revoked? | No mechanism | Revocation tracking |

### Regulatory Context

**EU AI Act (Articles 12 and 14):**

- Article 12: Automatic logging for post-market monitoring
- Article 14: Human oversight mechanisms
- **Mandates enable both**: Traceable authorization + audit trail

## Mandate Types

### 1. Intent Mandate (Standing Authority)

For ongoing, low-risk operations like browsing and discovery.

```yaml
# Example: "Let my agent search for products"
mandate_kind: intent
scope:
  tools: ["search_*", "list_*", "get_*"]
  operation_class: read
validity:
  expires_at: "2026-02-28T23:59:59Z"  # Valid for 1 month
```

**Use cases:**

- Product search and comparison
- Price monitoring
- Information gathering

### 2. Transaction Mandate (Final Authorization)

For specific, high-value actions requiring explicit consent.

```yaml
# Example: "Buy this specific item for up to $100"
mandate_kind: transaction
scope:
  tools: ["purchase_item"]
  operation_class: commit
  max_value:
    amount: "100.00"
    currency: "USD"
  transaction_ref: "sha256:cart_hash_abc123"
constraints:
  single_use: true
validity:
  expires_at: "2026-01-28T11:00:00Z"  # Valid for 1 hour
```

**Use cases:**

- Purchases and payments
- Account modifications
- Irreversible actions

## The Mandate Lifecycle

```mermaid
sequenceDiagram
    participant User
    participant App
    participant Agent
    participant Runtime as Assay Runtime
    participant Store

    Note over User,Store: 1. Authorization Phase
    User->>App: Grant permission
    App->>App: Create and Sign Mandate
    App->>Runtime: Submit mandate
    Runtime->>Store: Store mandate metadata

    Note over User,Store: 2. Execution Phase
    Agent->>Runtime: Tool call purchase_item
    Runtime->>Runtime: Check validity window
    Runtime->>Store: Check revocation status
    Runtime->>Store: Consume mandate atomic
    Store-->>Runtime: Receipt was_new=true
    Runtime->>Runtime: Emit mandate.used event
    Runtime-->>Agent: Authorized

    Note over User,Store: 3. Evidence Phase
    Runtime->>Runtime: Emit tool.decision event
    Agent->>Agent: Execute purchase
```

## Key Concepts

### Validity Windows

Mandates have explicit time boundaries:

| Field | Meaning |
|-------|---------|
| `not_before` | Earliest allowed use |
| `expires_at` | Latest allowed use |
| `issued_at` | When mandate was created |

**Clock tolerance:** The runtime allows 30 seconds of clock drift for `not_before` and `expires_at`.

### Revocation

Users can cancel mandates before they expire:

```mermaid
flowchart TD
    ACTIVE[Active Mandate] -->|user requests| REVOKE[Revocation Event]
    REVOKE --> STORED[Stored in revocation table]

    subgraph SubsequentCalls[Subsequent Tool Calls]
        CHECK{now >= revoked_at?}
        CHECK -->|Yes| REJECT[Reject]
        CHECK -->|No| ALLOW[Allow]
    end

    STORED --> CHECK
```

**Important:** Revocation has **no clock tolerance** - it takes effect immediately at `revoked_at`.

### Single-Use Protection

For high-value transactions, mandates can be limited to one use:

```mermaid
flowchart LR
    CALL1[First tool call] --> CONSUME[Consume mandate]
    CONSUME --> RECEIPT[Receipt use_count=1]

    CALL2[Second tool call] --> CHECK{use_count > 0?}
    CHECK -->|Yes| REJECT[AlreadyUsed Error]
```

**Idempotency:** If a tool call retries with the same `tool_call_id`, the same receipt is returned (no double-charging).

## Integration with Assay CLI

### Enabling Mandate Logging

```bash
assay mcp wrap \
  --policy assay.yaml \
  --audit-log audit.ndjson \
  --decision-log decisions.ndjson \
  --event-source "assay://myorg/myapp" \
  -- npx @modelcontextprotocol/server-filesystem
```

| Flag | Purpose |
|------|---------|
| `--audit-log` | Lifecycle events (mandate.used, mandate.revoked) |
| `--decision-log` | Tool decisions (allow/deny with reason codes) |
| `--event-source` | CloudEvents source URI (required for logging) |

### Policy Configuration

```yaml
# assay.yaml
mandate_trust:
  # Which tools require mandates
  commit_tools:
    - "purchase_*"
    - "transfer_*"
    - "payment_*"

  # Expected audience for mandates
  expected_audience: "myorg/myapp"

  # Trusted mandate issuers
  trusted_issuers:
    - "auth.myorg.com"

  # Clock tolerance for validity checks
  clock_skew_tolerance_seconds: 30
```

## Evidence Output

### mandate.used Event

Emitted when a mandate is consumed (first use only):

```json
{
  "specversion": "1.0",
  "type": "assay.mandate.used.v1",
  "id": "sha256:use_id_deterministic",
  "source": "assay://myorg/myapp",
  "time": "2026-01-28T10:05:00Z",
  "data": {
    "mandate_id": "sha256:abc123...",
    "use_id": "sha256:deterministic_hash",
    "tool_call_id": "tc_456",
    "use_count": 1
  }
}
```

**Note:** The event `id` equals `use_id`, enabling deduplication on retries.

### tool.decision Event

Every tool call produces a decision event:

```json
{
  "type": "assay.tool.decision",
  "data": {
    "tool": "purchase_item",
    "decision": "allow",
    "reason_code": "P_MANDATE_VALID",
    "tool_call_id": "tc_456",
    "mandate_id": "sha256:abc123...",
    "use_id": "sha256:...",
    "use_count": 1
  }
}
```

## Common Scenarios

### Scenario 1: Successful Purchase

```mermaid
sequenceDiagram
    participant User
    participant Agent
    participant Runtime

    User->>Agent: Buy the blue widget
    Note over Agent: Has transaction mandate<br/>max_value $100 single_use true

    Agent->>Runtime: purchase_item $45
    Runtime->>Runtime: Valid not expired
    Runtime->>Runtime: Not revoked
    Runtime->>Runtime: Within max_value
    Runtime->>Runtime: First use
    Runtime-->>Agent: Allow P_MANDATE_VALID
    Agent->>Agent: Execute purchase
```

### Scenario 2: Revoked Mandate

```mermaid
sequenceDiagram
    participant User
    participant Agent
    participant Runtime

    User->>Runtime: Revoke mandate
    Note over Runtime: revoked_at = now

    Agent->>Runtime: purchase_item $45
    Runtime->>Runtime: Mandate revoked
    Runtime-->>Agent: Deny M_REVOKED
```

### Scenario 3: Retry After Crash

```mermaid
sequenceDiagram
    participant Agent
    participant Runtime
    participant Store

    Agent->>Runtime: purchase_item tool_call_id=tc_001
    Runtime->>Store: Consume mandate
    Store-->>Runtime: Receipt was_new=true
    Runtime->>Runtime: Emit mandate.used
    Note over Runtime: Crash before response

    Agent->>Runtime: Retry purchase_item tool_call_id=tc_001
    Runtime->>Store: Consume mandate
    Store-->>Runtime: Receipt was_new=false
    Note over Runtime: No duplicate event
    Runtime-->>Agent: Allow same receipt
```

## Glossary

| Term | Definition |
|------|------------|
| **Mandate** | Cryptographically-signed user authorization |
| **Intent** | Standing authority for low-risk operations |
| **Transaction** | One-time authorization for specific action |
| **use_id** | Deterministic identifier for consumption receipt |
| **tool_call_id** | Unique identifier for a tool invocation |
| **Revocation** | Cancellation of mandate before expiry |
| **CloudEvents** | Standard event format for audit logs |

## Further Reading

- [SPEC-Mandate-v1](../architecture/SPEC-Mandate-v1.md) - Technical specification
- [ADR-017: Mandate Evidence](../architecture/ADR-017-Mandate-Evidence.md) - Architecture decision record
- [MCP Quickstart](../mcp/quickstart.md) - Getting started with MCP proxy
