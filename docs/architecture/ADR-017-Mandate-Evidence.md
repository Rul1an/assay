# ADR-017: Mandate/Intent Evidence

## Status

Accepted (January 2026, updated v1.0.3)

## Context

As AI agents move into agentic commerce and autonomous decision-making, a critical gap emerges: **proving user authorization for agent actions**. Current evidence bundles capture *what* an agent did, but not *who authorized it* or *within what scope*.

### The Authorization Problem

Traditional systems assume humans click buttons on trusted surfaces. With autonomous agents:

1. **Authorization Gap**: How do we prove a user granted specific authority for a purchase?
2. **Authenticity Gap**: How do we verify agent requests reflect actual user intent?
3. **Accountability Gap**: Who is responsible when transactions go wrong?

### Market Context (January 2026)

The agentic protocol landscape is fragmenting:

| Protocol | Owner | Focus |
|----------|-------|-------|
| **AP2** | Google/Coinbase | Agent payments with mandates |
| **UCP** | Google/Shopify | Commerce journeys |
| **ACP** | OpenAI/Stripe | Checkout flows |
| **A2A** | Google | Agent discovery/tasks |

All converge on one need: **verifiable proof of user intent** before autonomous actions.

### Regulatory Requirements

**EU AI Act Article 12 + 14:**
- Article 12: Automatic logging of events for post-market monitoring
- Article 14: Human oversight mechanisms
- Combined: Tool decisions should be traceable to human authorization

**AP2 Protocol (Sept 2025):**
> "Mandates are cryptographically-signed, tamper-proof digital contracts that serve as verifiable proof of a user's instructions."

### Current Assay State

Evidence Contract v1 captures:
- ✅ Tool calls with decisions (allow/deny)
- ✅ Policy evaluations
- ✅ W3C Trace Context correlation
- ❌ **No link to user authorization**
- ❌ **No mandate/intent provenance**

## Decision

We implement **Mandate Evidence** as a first-class evidence type that links tool calls to explicit user authorizations.

### Core Design Principles

1. **AP2-aligned lifecycle**: Distinguish `intent` (standing authority) from `transaction` (final authorization)
2. **Temporal precision**: Explicit `not_before`/`expires_at` timestamps, not vague TTL strings
3. **Consumption tracking**: `MandateUse` receipts for single-use enforcement
4. **Privacy-preserving**: Opaque principal identifiers, not PII
5. **Trust-anchored**: Reuse tool signing trust policy model
6. **Context-bound**: Prevent cross-context replay attacks

### Mandate Lifecycle

```
┌─────────────────┐      ┌─────────────────┐      ┌─────────────────┐
│  Intent Mandate │      │Transaction Mand.│      │  MandateUse     │
│  (standing)     │─────▶│  (final)        │─────▶│  (receipt)      │
└────────┬────────┘      └────────┬────────┘      └────────┬────────┘
         │                        │                        │
         ▼                        ▼                        ▼
   ┌───────────┐           ┌───────────┐           ┌───────────┐
   │ Discovery │           │  Commit   │           │ Evidence  │
   │ Read-only │           │ Purchase  │           │  Bundle   │
   │ Tool Calls│           │ Tool Calls│           │           │
   └───────────┘           └───────────┘           └───────────┘
```

**Mandate Kinds:**

| Kind | Purpose | Allowed Operations |
|------|---------|-------------------|
| `intent` | Standing authority | Discovery, read-only, browsing |
| `transaction` | Final authorization | Commit, purchase, write, transfer |
| `revocation` | Cancel existing mandate | N/A (administrative) |

### Event Types

| Event Type | Purpose |
|------------|---------|
| `assay.mandate.v1` | Mandate grant (intent or transaction) |
| `assay.mandate.used.v1` | Consumption receipt for single-use tracking |
| `assay.tool.decision` | Extended with `mandate_id` linkage |

## Mandate Schema

### Normative Definitions

**mandate_id computation (MUST):**

```
mandate_id = "sha256:" + hex(SHA256(JCS(mandate_content_without_signature)))
```

Where:
- `JCS` = [RFC 8785 JSON Canonicalization Scheme](https://tools.ietf.org/html/rfc8785)
- `mandate_content_without_signature` = the `data` object excluding the `signature` field
- `payload_digest` in signature block MUST equal `mandate_id`

This ensures **one source of truth**: verifiers check `mandate_id == signature.payload_digest == digest(canonical payload)`.

### assay.mandate.v1

```json
{
  "type": "assay.mandate.v1",
  "data": {
    "mandate_id": "sha256:abc123...",
    "mandate_kind": "intent | transaction | revocation",

    "principal": {
      "subject": "opaque-subject-id",
      "method": "oidc | did | spiffe | local_user | service_account",
      "display": "Optional display name (UX only, MUST NOT use for verification)",
      "credential_ref": "sha256:... (see Credential Reference below)"
    },

    "scope": {
      "tools": ["read_*", "search_*"],
      "resources": ["/products/**"],
      "operation_class": "read",
      "max_value": {
        "amount": "100.00",
        "currency": "USD"
      }
    },

    "validity": {
      "not_before": "2026-01-28T10:00:00Z",
      "expires_at": "2026-01-28T11:00:00Z",
      "issued_at": "2026-01-28T09:55:00Z"
    },

    "constraints": {
      "single_use": false,
      "max_uses": null,
      "require_confirmation": false
    },

    "context": {
      "audience": "myorg/myapp",
      "issuer": "auth.myorg.com",
      "nonce": "session-abc123",
      "traceparent": "00-0af7651916cd43dd8448eb211c80319c-..."
    },

    "signature": {
      "version": 1,
      "algorithm": "ed25519",
      "payload_type": "application/vnd.assay.mandate+json;v=1",
      "payload_digest": "sha256:abc123...",
      "key_id": "sha256:789xyz...",
      "signature": "base64...",
      "signed_at": "2026-01-28T09:55:00Z"
    }
  }
}
```

### Field Semantics

**scope.operation_class** (enum):

| Class | Description | Example Tools |
|-------|-------------|---------------|
| `read` | Discovery, browsing, read-only | `search_*`, `list_*`, `get_*` |
| `write` | Modifications, non-financial | `update_*`, `fs.write_*` |
| `commit` | Financial transactions, irreversible | `purchase_*`, `transfer_*`, `order_*` |

**scope.max_value** (struct, nullable):

```json
{
  "amount": "100.00",   // Decimal as string (no floats!)
  "currency": "USD"     // ISO 4217
}
```

**constraints.max_uses** semantics:

| Value | Meaning |
|-------|---------|
| `null` | Unlimited uses (default) |
| `1` | Single use (equivalent to `single_use: true`) |
| `N` | Maximum N uses |

Note: `single_use: true` is syntactic sugar for `max_uses: 1`.

**principal.credential_ref** format:

```
"sha256:" + hex(SHA256(credential_bytes))
```

Where `credential_bytes` is:
- For JWT VP: the raw UTF-8 bytes of the compact JWT
- For JSON VP: the JCS-canonicalized bytes
- v1: Opaque string, MUST be stable within organization
- v2: Will specify normative canonicalization per credential format

### assay.mandate.used.v1

```json
{
  "type": "assay.mandate.used.v1",
  "data": {
    "mandate_id": "sha256:abc123...",
    "use_id": "sha256:use789...",
    "tool_call_id": "tc_456",
    "consumed_at": "2026-01-28T10:05:00Z",
    "use_count": 1
  }
}
```

### assay.mandate.revoked.v1

```json
{
  "type": "assay.mandate.revoked.v1",
  "data": {
    "mandate_id": "sha256:abc123...",
    "revoked_at": "2026-01-28T10:30:00Z",
    "reason": "user_requested | admin_override | policy_violation | expired_early",
    "revoked_by": "opaque-subject-id"
  }
}
```

**Revocation semantics:**

| Aspect | Behavior |
|--------|----------|
| **Effect** | Mandate MUST NOT be used after `revoked_at` |
| **Retroactivity** | NOT retroactive; uses before `revoked_at` remain valid |
| **Ordering** | Runtime: `now >= revoked_at` → reject; Lint: compare `tool.decision.time` vs `revoked_at` |
| **Propagation** | Revocation applies only to the specified mandate, not derived/delegated mandates (v2) |

### Tool Decision Extension

```json
{
  "type": "assay.tool.decision",
  "data": {
    "tool": "purchase_item",
    "decision": "allow",
    "reason_code": "P_MANDATE_VALID",
    "args_schema_hash": "sha256:...",
    "tool_call_id": "tc_456",
    "mandate_id": "sha256:abc123...",
    "mandate_scope_match": true
  }
}
```

## Trust Model

### Signature Trust (Reuse Tool Signing)

Mandate signatures use the **same trust policy model** as tool signing ([SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)):

```yaml
# assay.yaml or policy.yaml
mandate_trust:
  require_signed: true
  trusted_key_ids:
    - sha256:abc123...  # Production signing key
    - sha256:def456...  # CI signing key
  allow_embedded_key: false  # Dev only
```

**Verification flow:**
1. Extract `signature.key_id` from mandate
2. Check if `key_id` is in `trusted_key_ids`
3. Verify Ed25519 signature over DSSE PAE envelope
4. Reject if untrusted or invalid

### Context Binding (Replay Prevention)

The `context` block prevents mandate reuse across environments:

| Field | Purpose | Binding Scope |
|-------|---------|---------------|
| `audience` | Target application/org | MUST match runtime `expected_audience` |
| `issuer` | Signing authority | MUST be in `trusted_issuers` allowlist |
| `nonce` | Session binding | Real-time/interactive flows only |
| `traceparent` | W3C Trace Context | Correlation, not security |

**Audience determination (runtime):**

```yaml
# assay.yaml
mandate_trust:
  expected_audience: "myorg/myapp"  # or from env: ${ASSAY_AUDIENCE}
  trusted_issuers:
    - "auth.myorg.com"
    - "idp.partner.com"
```

**Verification rules (normative):**

1. `mandate.context.audience == config.expected_audience` → PASS
2. `mandate.context.issuer IN config.trusted_issuers` → PASS
3. If `nonce` present: verify against session store (implementation-specific)

**Nonce guidance:**

| Mandate Kind | Nonce Use |
|--------------|-----------|
| `intent` (standing) | Optional; prefer `audience` + `issuer` + `scope` hash |
| `transaction` (final) | Recommended for interactive confirmation flows |
| `revocation` | Not applicable |

Standing mandates with long validity SHOULD NOT rely solely on nonce (which implies session binding). Instead, context binding via `audience` + `issuer` + `scope` provides sufficient replay prevention.

## Time Semantics

### Normative Time Source

| Context | Time Source | Use |
|---------|-------------|-----|
| **Runtime** | Wall clock (`Utc::now()`) | Authorization check before tool execution |
| **Lint** | Event `time` field | Forensic verification post-hoc |

**Runtime behavior:**
```rust
fn check_mandate_validity(mandate: &Mandate, now: DateTime<Utc>) -> Result<()> {
    if let Some(nb) = mandate.validity.not_before {
        if now < nb {
            return Err(MandateError::NotYetValid);
        }
    }
    if let Some(exp) = mandate.validity.expires_at {
        if now >= exp {
            return Err(MandateError::Expired);
        }
    }
    Ok(())
}
```

**Lint behavior:**
- Uses CloudEvents `time` field from tool decision event
- Compares: `not_before <= event.time < expires_at`
- Forensic: detects violations post-hoc, not runtime guarantee

## Single-Use Enforcement

### The Concurrency Problem

Pure log-based systems cannot atomically enforce single-use:
- Two parallel tool calls may both appear "first"
- Without shared state, enforcement is best-effort

### Solution: MandateUse Receipts

1. **Runtime**: Atomic check in local store (SQLite) before tool execution
2. **Evidence**: `assay.mandate.used.v1` event records consumption
3. **Lint**: Detects violations via `use_count` analysis

### Runtime Implementation (v1.0.3)

See [SPEC-Mandate-v1 §7](./SPEC-Mandate-v1.md#7-runtime-enforcement-normative) for full normative specification.

**Key design decisions:**

| Decision | Choice | Rationale |
|----------|--------|-----------|
| Storage | SQLite + WAL | Atomic transactions, crash recovery, no external deps |
| Idempotency | `tool_call_id UNIQUE` constraint | Retry-safe, no double-increment |
| Nonce check | `INSERT` (not SELECT+INSERT) | Race-condition proof |
| Crash semantics | Consume-before-exec | Single-use guarantee > execution guarantee |
| Clock skew | Widened window (±30s default) | Tolerant but auditable |

**MandateStore interface:**

```rust
pub struct MandateStore {
    conn: Arc<Mutex<Connection>>,  // SQLite with WAL
}

impl MandateStore {
    /// Upsert mandate metadata (immutable after first insert)
    pub async fn upsert_mandate(&self, mandate: &Mandate) -> Result<()>;

    /// Atomic consume with idempotency on tool_call_id
    pub async fn consume_mandate(
        &self,
        mandate_id: &str,
        tool_call_id: &str,
        nonce: Option<&str>,
        audience: &str,
        issuer: &str,
        single_use: bool,
        max_uses: Option<u32>,
        tool_name: &str,
        operation_class: OperationClass,
    ) -> Result<AuthzReceipt, AuthzError>;
}
```

**Invariants (MUST):**

- Same `tool_call_id` → same receipt (idempotent)
- `single_use=true` + `use_count>0` → `AlreadyUsed` error
- `use_count >= max_uses` → `MaxUsesExceeded` error
- Duplicate nonce (same audience+issuer) → `NonceReplay` error
- `mandate.used` event MUST be emitted before tool execution
- `tool.decision` event MUST be emitted even on execution failure
```

## Pack Rules

### mandate-baseline.yaml

| Rule ID | Check | Severity | Scope |
|---------|-------|----------|-------|
| `MANDATE-001` | `decision=allow` for commit tools MUST have `mandate_id` | error | `commit_tools` only |
| `MANDATE-002` | `mandate_id` MUST reference existing `assay.mandate.v1` in bundle | error | all |
| `MANDATE-003` | Tool call timestamp within `not_before`..`expires_at` | error | all |
| `MANDATE-004` | `single_use`/`max_uses` mandate has valid receipt count | error | all |
| `MANDATE-005` | `mandate_kind=transaction` required for commit tools | warning | `commit_tools` |

**Note on false positive minimization:** MANDATE-001 only applies to tools classified as `commit` (via `mandate_trust.commit_tools` config). Read-only discovery flows do not require mandate linkage, preventing adoption friction.

### Tool Classification Config

```yaml
# assay.yaml
mandate_trust:
  commit_tools:
    - "purchase_*"
    - "transfer_*"
    - "order_*"
    - "payment_*"
  write_tools:
    - "update_*"
    - "edit_*"
    - "fs.write_*"
    - "fs.delete_*"
```

### EU AI Act Integration

Add to `eu-ai-act-baseline.yaml`:

```yaml
- id: EU12-005
  article_ref: ["12", "14"]
  short_id: EU12-005
  description: "Tool decisions should link to human authorization"
  check:
    type: json_path_exists
    paths:
      - "/data/mandate_id"
    event_types:
      - "assay.tool.decision"
  severity: warning
  help_markdown: |
    ## EU AI Act Articles 12 & 14 - Authorization Traceability

    This check verifies that tool decisions can be traced to human
    authorization via mandate_id references.

    **Note:** Not all workflows require explicit mandates. This is a
    progressive requirement based on risk classification.
```

## Evidence Bundle Structure

All mandate data stored in `events.ndjson` (no separate files):

```
bundle.tar.gz
├── manifest.json
└── events.ndjson
    ├── assay.mandate.v1         # Mandate grants
    ├── assay.mandate.used.v1    # Consumption receipts
    ├── assay.tool.decision      # Tool calls with mandate_id
    └── ...
```

**Rationale:** Keeps verification simple. `verify_bundle()` already validates `events.ndjson` integrity via content hash in manifest.

## Consequences

### Positive

- **Verifiable Authorization**: Cryptographic proof of user intent
- **AP2 Compatibility**: Direct alignment with emerging commerce protocols
- **EU AI Act Aligned**: Enables technical traceability signals aligned with Article 12+14 requirements
- **Privacy-Preserving**: Opaque principal IDs, no PII in evidence
- **Trust Reuse**: Same key management as tool signing

### Negative

- **Schema Extension**: New event types require version bump consideration
- **Runtime Overhead**: Mandate validation adds latency to tool calls
- **Storage Growth**: MandateUse receipts increase bundle size

### Risks

| Risk | Mitigation |
|------|------------|
| Principal PII leakage | Use opaque `subject`, not email/names |
| Clock skew issues | Document normative time semantics clearly |
| Single-use race conditions | Atomic store operations + receipts |
| Cross-context replay | `context.audience` + `issuer` binding |

## Alternatives Considered

### 1. Inline Authorization in Tool Calls

**Rejected.** Duplicates authorization data in every tool call, no single source of truth.

### 2. External Mandate Service

**Rejected.** Adds external dependency, breaks offline verification, BYOS philosophy.

### 3. Simple Token Reference

**Rejected.** No cryptographic proof, no scope validation, no temporal validity.

## V2 Roadmap

| Feature | Description |
|---------|-------------|
| OpenID4VP binding | VP hash in `credential_ref` |
| Sigstore keyless | Fulcio + Rekor transparency log |
| Transaction mandate | Cart hash, line items, currency+amount |
| Delegation chains | Mandate-to-mandate delegation |

## References

- [SPEC-Mandate-v1](./SPEC-Mandate-v1.md) - Detailed technical specification
- [AP2 Protocol](https://agentpaymentsprotocol.info/specification/) - Agent Payments mandates
- [EU AI Act Article 12](https://artificialintelligenceact.eu/article/12/) - Record-keeping
- [EU AI Act Article 14](https://artificialintelligenceact.eu/article/14/) - Human oversight
- [OpenID4VP 1.0](https://openid.net/specs/openid-4-verifiable-presentations-1_0.html) - Verifiable presentations
- [DSSE Specification](https://github.com/secure-systems-lab/dsse) - Signing envelope
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785) - JCS for mandate_id
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md) - Assay tool signing (reused for mandates)
- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md) - Base evidence schema
