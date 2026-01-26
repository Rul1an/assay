# ADR-006: Evidence Contract for Agent Runtime

## Status
Proposed (Q1 2026 Strategy)

## Context
As agents move toward production, auditability and governance become primary requirements. Current logging is often non-standard and difficult to integrate with Enterprise security stacks. We need a first-class "Evidence Contract" that is tamper-evident, standardized, and interoperable.

## Decision
Assay will adopt a multi-layer standardized Evidence Format.

### 1. Envelope: CloudEvents v1.0
All evidence events will follow the [CloudEvents](https://cloudevents.io/) specification. This ensures compatibility with SIEM/SOAR and event-driven architectures.

| Field | Value / Description |
| :--- | :--- |
| `specversion` | "1.0" |
| `type` | e.g., `assay.sandbox.v1.decision` |
| `source` | URI of the assay runner/instance |
| `subject` | e.g., `tool.read_file` |
| `id` | Deterministic Event ID (see ADR-007) |
| `time` | RFC 3339 Timestamp (UTC) |

### 2. Context: OpenTelemetry (OTel)
To enable correlation across the agentic stack, all events must carry OTel trace and span IDs.

```json
{
  "trace_id": "...",
  "span_id": "...",
  "parent_span_id": "..."
}
```

### 3. Payload: Domain-Specific Evidence
Specific event types for Assay operations:
- **`assay.sandbox.started/finished`**: Metadata about the environment (OS, Landlock ABI, process).
- **`assay.policy.decision`**: The core "Trust Event". Includes `code`, `reason`, `contract`, and `decision` (Allow/Deny/Partial).
- **`assay.tool.invoked/result`**: Tool execution evidence. Includes `schema_hash` and `redacted_args`.
- **`assay.integrity.failure`**: Special event for Tool Drift or Supply-Chain issues.

### 4. Data Privacy Classes
The format enforces strict redaction categories:
- **`CLASS_PUBLIC`**: Meta-hash, tool name, timestamps. Always logged.
- **`CLASS_SENSITIVE`**: Arguments, environment variables. Redacted/masked by default.
- **`CLASS_FORBIDDEN`**: Secrets, tokens. Never recorded in evidence.

## Consequences
- Assay becomes interoperable with established observability and security stacks (OTel/SIEM).
- The Evidence Format is the "Protocol" that connects the Open Source runner to the Paid Evidence Store.
- High performance cost for canonicalization and hashing (offset by Blake3 speed).
