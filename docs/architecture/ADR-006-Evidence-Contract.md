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
| `type` | string | Event type identifier | e.g. `assay.env.filtered`, `assay.tool.decision`. |
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
| `assaycontenthash` | string | **Content Integrity** | `sha256` over the v1 canonical content-hash input. |
| `data` | object | **Type-Specific Data** | Validated against `type` schema. |

In v1, `assaycontenthash` covers the JCS-canonicalized content-hash input:
`specversion`, `type`, `datacontenttype`, optional `subject`, and `data`.
It does not cover the full CloudEvents envelope, stream identity, timestamp,
producer metadata, policy metadata, privacy flags, or trace context. This is
intentionally narrower than the full envelope but not data-only: changing the
event type changes the content hash.

### 2. Privacy Classes (Data Protection)

The format enforces strict redaction categories to ensure evidence is "safe by default" for storage.

| Class | Description | Handling | Examples |
| :--- | :--- | :--- | :--- |
| **`public`** | Metadata, hashes, timestamps | Always logged | `event_type`, `run_id`, `tool_name` |
| **`sensitive`** | Arguments, paths, env output | **Generalized** | `/Users/name/file` -> `~/**/file`, `--token=xyz` -> `--token=***` |
| **`forbidden`** | Secrets, Tokens, PII | **Dropped** completely | `Authorization` headers, raw secret values |

### 3. Core Payload Schemas (v1.0)

Core payloads are mapped from `assay-cli` into the Evidence envelope. Some
payloads also have convenience Rust types in `assay-evidence`; the v1 envelope
and content-hash contract remain generic over JSON payloads.

<a id="payload-assay-profile-started"></a>
<a id="payload-assay-profile-finished"></a>
#### A. `assay.profile.started` / `assay.profile.finished` (Run Context)
Records the start and end of a profile evidence export.

`assay.profile.started`:
```json
{
  "profile_name": "string",
  "profile_version": "string",
  "total_runs_aggregated": 50
}
```

`assay.profile.finished`:
```json
{
  "files_count": 1,
  "network_count": 1,
  "processes_count": 1,
  "sandbox_degradation_count": 0,
  "integrity_scope": "observed"
}
```

`integrity_scope` is optional and records the profile/export scope when present.

<a id="payload-assay-tool-decision"></a>
#### B. `assay.tool.decision` (Policy Enforcement)
Records authorization decisions (HITL-ready, protocol-based).
```json
{
  "tool": "read_file",
  "decision": "allow|deny|requires_approval",
  "reason_code": "E_POLICY_DENY",
  "args_schema_hash": "sha256:...",
  "policy_digest": "sha256:...",
  "policy_snapshot_digest": "sha256:...",
  "policy_snapshot_digest_alg": "sha256",
  "policy_snapshot_canonicalization": "jcs:mcp_policy",
  "policy_snapshot_schema": "assay.mcp.policy.snapshot.v1",
  "tool_definition_digest": "sha256:...",
  "tool_definition_digest_alg": "sha256",
  "tool_definition_canonicalization": "jcs:mcp_tool_definition.v1",
  "tool_definition_schema": "assay.mcp.tool-definition.snapshot.v1",
  "tool_definition_source": "mcp.tools/list",
  "delegated_from": "agent:planner",
  "delegation_depth": 1
}
```

`policy_snapshot_digest`, `policy_snapshot_digest_alg`,
`policy_snapshot_canonicalization`, and `policy_snapshot_schema` are additive
optional P56a fields. They make the canonical MCP policy snapshot digest
visible when supported runtime decision paths already have a policy digest.
They do not imply that the policy is correct, sufficient, safe, approved, or
complete. Existing `policy_digest` remains a compatibility field;
`policy_snapshot_digest` is the explicit reviewer surface. On supported paths,
`policy_snapshot_digest` is the self-describing projection of `policy_digest`
and MUST carry the same digest value. If `policy_snapshot_digest` is present,
then `policy_snapshot_digest_alg`, `policy_snapshot_canonicalization`, and
`policy_snapshot_schema` MUST also be present.

`policy_snapshot_canonicalization: "jcs:mcp_policy"` means the digest is
computed over the existing canonical serialization of the `McpPolicy` object
using JCS before SHA-256 hashing, matching the `McpPolicy::policy_digest()`
implementation. P56a projects this existing digest only; it does not infer or
reconstruct policy snapshots after the fact, and it does not make policy
snapshots retrievable, exportable, or embedded. Absence of
`policy_snapshot_digest` means the policy snapshot boundary is not visible, not
that the decision is safe.

`tool_definition_digest`, `tool_definition_digest_alg`,
`tool_definition_canonicalization`, `tool_definition_schema`, and
`tool_definition_source` are additive optional P56b fields. They make the
bounded MCP `tools/list` tool-definition digest visible when a supported
decision path observed that definition before the tool call. The field cluster
is atomic: if `tool_definition_digest` is present, the algorithm,
canonicalization, schema, and source fields MUST also be present.

For P56b v1, `tool_definition_digest_alg` is `"sha256"`,
`tool_definition_canonicalization` is `"jcs:mcp_tool_definition.v1"`,
`tool_definition_schema` is `"assay.mcp.tool-definition.snapshot.v1"`, and
`tool_definition_source` is `"mcp.tools/list"`. The digest is computed over a
bounded JCS projection of the observed tool definition: `name`, optional
trimmed `description`, optional full normalized `input_schema`, and optional
`server_id` only when the observed definition is server-scoped. Top-level
vendor/provider metadata, annotations, display hints, runtime result payloads,
registry bodies, and `x-assay-sig` are excluded before digesting. Schema
keywords inside `input_schema` remain part of the reviewed schema surface.
P56b does not claim tool safety, implementation truth, signature validity,
signer trust, registry trust, or that the tool definition is retrievable or
embedded. Absence of `tool_definition_digest` means the tool-definition
boundary is not visible, not that the tool is safe.

`delegated_from` and `delegation_depth` are additive optional fields. They are
surfaced only when a supported decision flow carries explicit
`_meta.delegation` context. They do not imply delegation-chain completeness or
integrity, inherited-scope validation, or temporal correctness.

<a id="payload-assay-sandbox-degraded"></a>
#### C. `assay.sandbox.degraded` (Operational Integrity)
Records when stronger-than-audit containment was requested, weaker containment
became effective, and execution continued.
```json
{
  "reason_code": "policy_conflict",
  "degradation_mode": "audit_fallback",
  "component": "landlock",
  "detail": "optional, redacted operator context"
}
```

`reason_code` is currently `backend_unavailable` or `policy_conflict`.
`degradation_mode` is currently `audit_fallback`. `component` is currently
`landlock`. `detail` is optional and must be redacted operator context.

<a id="payload-assay-fs-access"></a>
#### D. `assay.fs.access` (Activity Log)
Records filesystem activity with generalized paths. In observed mode, payloads
are minimized and the generalized subject is carried in the envelope `subject`.
```json
{
  "hits": 3
}
```

Full-detail local exports may include additional observed fields such as
`file`, `first_seen`, `last_seen`, and `runs_seen`. Those fields are additive
and not required for v1 stable consumption.

<a id="payload-assay-env-filtered"></a>
#### E. `assay.env.filtered` (Environment Filtering)
Records environment-filtering posture and summary counts without raw
environment values.
```json
{
  "mode": "strict",
  "passed_keys": ["PATH"],
  "dropped_keys": ["OPENAI_API_KEY"],
  "counters": {
    "passed": 1,
    "dropped": 1
  }
}
```

`mode` is the observed filter mode. `passed_keys` and `dropped_keys` are key
names only; raw environment values must not appear in this payload. `counters`
is an additive map of summary counters.

## Consequences
- **Interoperability**: Standard envelope allows ingestion by any CloudEvents-compatible system (Splunk, Azure Event Grid).
- **Audit-Ready**: Separation of `sensitive` data ensures evidence can be stored long-term without GDPR/compliance risks.
- **Strictness**: Breaking changes to schemas require new `type` versions (e.g. `assay.env.filtered.v2`).
