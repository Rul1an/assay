# ADR-006: Evidence Contract for Agent Runtime

## Status
**Adopted** (Q1 2026 Strategy)

## Context
As agents move toward production, auditability and governance become primary requirements. Current logging is often non-standard and difficult to integrate with Enterprise security stacks. We need a first-class "Evidence Contract" that is tamper-evident, standardized, and interoperable.

## Decision
Assay will adopt a multi-layer standardized Evidence Format based on **CloudEvents v1.0** and **OpenTelemetry (OTel)** correlation.

### 1. Evidence Envelope (CloudEvents v1.0-style)

Every evidence record is an Event enveloping a type-specific Payload.

**Schema:** `assay.evidence.event.v1`

| Field | Type | Description | Invariants |
| :--- | :--- | :--- | :--- |
| `specversion` | `1.0` | CloudEvents spec version | Fixed string. |
| `type` | string | Event Type URN | e.g. `assay.env.filtered`, `assay.tool.decision`. |
| `source` | string | Producer Identifier | URI identifying the specific runner instance. |
| `id` | string | Event ID | `{run_id}:{seq}` (e.g. `run_abc:0`). |
| `time` | string | Timestamp (RFC3339) | UTC only. |
| `subject` | string | Subject ID (optional) | Semantic subject (e.g. `tool:read_file`, `policy:check`). |
| `traceparent` | string | W3C Trace Parent | Required for correlation. |
| `tracestate` | string | W3C Trace State | Optional. |
| `assayrunid` | string | Run Context (Flattened) | Deterministic ID for the run. |
| `assayseq` | int | Sequence (Flattened) | 0-indexed monotonic counter. |
| `assayproducer` | string | Producer Name | e.g. "assay". |
| `assayproducerversion`| string | Producer Version | e.g. "2.6.0". |
| `assaycontenthash` | string | **Payload Integrity** | `sha256(canonical_payload)`. |
| `data` | object | **Type-Specific Data** | Validated against `type` schema. |

### 2. Privacy Classes (Data Protection)

The format enforces strict redaction categories to ensure evidence is "safe by default" for storage.

| Class | Description | Handling | Examples |
| :--- | :--- | :--- | :--- |
| **`public`** | Metadata, hashes, timestamps | Always logged | `event_type`, `run_id`, `tool_name` |
| **`sensitive`** | Arguments, paths, env output | **Redacted** or Generalized | `path: /home/user/...` -> `~/***`, `env: AWS_KEY` -> `***` |
| **`forbidden`** | Secrets, Tokens, PII | **Dropped** completely | `OPENAI_API_KEY` values, Authorization headers |

### 3. Core Payload Schemas (v1)

All payloads must be defined via stable JSON Schemas.

#### A. `env.filtered` (Environment Hygiene)
Records environment scrubbing results without leaking values.
```json
{
  "mode": "strict|scrub|passthrough",
  "passed_keys": ["PATH", "HOME"],
  "dropped_keys": ["AWS_SECRET_ACCESS_KEY"],
  "counters": { "passed": 2, "dropped": 1 }
}
```

#### B. `tool.decision` (Policy Enforcement)
Records authorization decisions (HITL-ready, protocol-based).
```json
{
  "tool": "read_file",
  "decision": "allow|deny|requires_approval",
  "reason_code": "E_POLICY_DENY",
  "args_schema_hash": "sha256:..."
}
```

#### C. `sandbox.degraded` (Operational Integrity)
Records when security guarantees are weakened.
```json
{
  "reason_code": "E_POLICY_CONFLICT_DENY_WINS_UNENFORCEABLE",
  "message": "Degrading to Audit mode due to conflict on non-Linux platform."
}
```

#### D. `fs.observed` (Activity Log)
Records filesystem activity with generalized paths.
```json
{
  "op": "read|write|exec",
  "path": "${ASSAY_TMP}/input.txt",
  "backend": "landlock|ebpf"
}
```

## Consequences
- **Interoperability**: Standard envelope allows ingestion by any CloudEvents-compatible system (Splunk, Azure Event Grid).
- **Audit-Ready**: Separation of `sensitive` data ensures evidence can be stored long-term without GDPR/compliance risks.
- **Strictness**: Breaking changes to schemas require new `type` versions (e.g. `assay.env.filtered.v2`).
