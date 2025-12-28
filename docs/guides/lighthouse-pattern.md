# The Lighthouse Pattern: Enterprise Pilot Architecture

This guide documents the reference architecture for the "Lighthouse Pilot" configuration, designed for high-stakes enterprise deployments (e.g., Autonomous Accounts Payable).

## 1. Architecture Overview

The Lighthouse Pattern positions Assay as an authoritative but non-blocking "Decision Gateway" in the early phases of deployment.

```mermaid
graph TD
    User((Finance Manager)) -->|Approves Task| Agent[Celonis AgentC]

    subgraph "Trust Layer (Invisible to User)"
        Agent -->|1. Tool Call (Approve Invoice)| Assay{Assay Guardrail}

        Assay -->|Policy Check < 0.1ms| PolicyDB[(Compliance Rules)]

        Assay -- "❌ BLOCK (Risk > €500)" --> Agent
        Assay -- "✅ ALLOW" --> EMS[Backend System]
    end

    Agent -->|2. Feedback Loop| User
    Assay -.->|3. Audit Log| Datadog[Audit Trail]

    style Assay fill:#00d97e,stroke:#333,stroke-width:2px,color:white
```

## 2. Configuration Strategy

For the pilot phase, we utilize a "Fail-Open with Warning" strategy to ensure business continuity while gathering data.

### Fail-Safe Mode (`on_error: allow`)

Configure the MCP server to allow operations even if the policy engine experiences catastrophic failure (e.g., config corruption), but explicitly warn the agent.

**Client Request:**
```json
{
  "method": "tools/call",
  "params": {
    "name": "approve_invoice",
    "arguments": {
      "amount": 500,
      "on_error": "allow",
      "policy": "finance_policy_v1"
    }
  }
}
```

**System Response (on Failure):**
```json
{
  "content": [...],
  "isError": false,
  "warning": "FAIL-SAFE ACTIVE: Policy engine offline. Proceed with caution (Safe Mode)."
}
```

## 3. Telemetry & Billing

Assay emits structured logs for both Operational Monitoring and Metered Billing.

### Metered Billing Event
Ingest these logs to calculate "Premium Governance" usage.

```json
{
  "target": "assay_billing",
  "event": "assay.usage.metered",
  "usage_type": "policy_check",
  "count": 1
}
```

### Fail-Safe Alert
Trigger P1 alerts on this event.

```json
{
  "event": "assay.failsafe.triggered",
  "error": "...",
  "fallback": "allow"
}
```
