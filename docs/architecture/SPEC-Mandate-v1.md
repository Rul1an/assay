# Mandate Evidence Specification v1

**Status:** Draft v1.0.2 (January 2026)
**Scope:** Cryptographically-signed user authorization evidence for AI agent tool calls
**ADR:** [ADR-017: Mandate/Intent Evidence](./ADR-017-Mandate-Evidence.md)

**Changelog:**
- v1.0.2: Fixed payload_digest semantics (DSSE alignment), removed mandate_kind=revocation, added conformance test vectors, normative transaction_ref schema, require_signed_lifecycle default for commit
- v1.0.1: Fixed mandate_id circularity, added lifecycle event trust model, normative glob semantics, operation_class ordering

---

## 1. Overview

This specification defines the mandate evidence format for proving user authorization of AI agent actions. Mandates are cryptographically-signed, tamper-proof records that link tool decisions to explicit user intent.

### Design Principles

- **AP2-aligned** - Compatible with emerging agent commerce protocols (AP2, UCP, ACP)
- **Deterministic** - Same mandate content always produces same `mandate_id`
- **Offline-verifiable** - Verification requires only trusted keys, no network
- **Privacy-preserving** - Opaque principal identifiers, no PII
- **DSSE-compatible** - Uses same signing envelope as tool signing

### Mandate Kinds

| Kind | Purpose | Allowed Operation Classes |
|------|---------|---------------------------|
| `intent` | Standing authority for discovery/browsing | `read` |
| `transaction` | Final authorization for commits/purchases | `read`, `write`, `commit` |

> **Note (v1.0.2):** `revocation` was removed as a mandate kind. Revocation is handled exclusively via `assay.mandate.revoked.v1` events. This simplifies the model: mandates authorize, events record lifecycle transitions.

---

## 2. Normative Definitions

### 2.1 mandate_id Computation (MUST)

```
mandate_id = "sha256:" + lowercase_hex(SHA256(JCS(hashable_content)))
```

Where:
- `JCS` = [RFC 8785 JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785)
- `hashable_content` = the `data` object **excluding both** `mandate_id` **and** `signature` fields
- The result is a 71-character string: `sha256:` (7 chars) + 64 hex chars

**Critical:** The `mandate_id` is computed from content that does NOT include `mandate_id` itself. This avoids circularity and ensures implementations in any language produce identical IDs.

**Normative example:**

```json
// Step 1: Build hashable_content (WITHOUT mandate_id and signature):
{
  "mandate_kind": "intent",
  "principal": { "subject": "user-123", "method": "oidc" },
  "scope": { "tools": ["search_*"], "operation_class": "read" },
  "validity": { "issued_at": "2026-01-28T10:00:00Z" },
  "constraints": {},
  "context": { "audience": "myorg/app", "issuer": "auth.myorg.com" }
}

// Step 2: JCS canonical form (single line, sorted keys):
{"constraints":{},"context":{"audience":"myorg/app","issuer":"auth.myorg.com"},"mandate_kind":"intent","principal":{"method":"oidc","subject":"user-123"},"scope":{"operation_class":"read","tools":["search_*"]},"validity":{"issued_at":"2026-01-28T10:00:00Z"}}

// Step 3: Compute mandate_id = "sha256:" + hex(SHA256(canonical_bytes))
// Step 4: Set data.mandate_id = computed mandate_id
// Step 5: Proceed to signing (which signs the full content including mandate_id)
```

**Digest semantics (v1.0.2):**

The signature object contains TWO digest fields:

| Field | Computed From | Purpose |
|-------|---------------|---------|
| `content_id` | `JCS(hashable_content)` without mandate_id/signature | Content-addressed identifier = `mandate_id` |
| `signed_payload_digest` | `JCS(signable_content)` with mandate_id, without signature | Standard DSSE payload digest |

**Binding rule:** Verifiers MUST check BOTH:

```
1. mandate_id == signature.content_id == "sha256:" + hex(SHA256(JCS(content_without_mandate_id_and_signature)))
2. signature.signed_payload_digest == "sha256:" + hex(SHA256(JCS(content_with_mandate_id_but_without_signature)))
```

This separates the content-addressed identifier (for lookups/references) from the signed payload digest (for DSSE verification), avoiding implementer confusion.

### 2.2 Operation Classes (Normative Ordering)

**Normative ordering:** `read` < `write` < `commit`

| Class | Ordinal | Description | Example Tools | Mandate Kind Required |
|-------|---------|-------------|---------------|----------------------|
| `read` | 0 | Discovery, browsing, read-only | `search_*`, `list_*`, `get_*` | `intent` or `transaction` |
| `write` | 1 | Modifications, non-financial | `update_*`, `fs.write_*`, `edit_*` | `intent` or `transaction` |
| `commit` | 2 | Financial transactions, irreversible | `purchase_*`, `transfer_*`, `order_*` | `transaction` only |

**Highest-allowed semantics:**

When a mandate specifies `operation_class`, it authorizes that class **and all lower classes**:
- `operation_class: "commit"` → allows `read`, `write`, `commit`
- `operation_class: "write"` → allows `read`, `write` (NOT `commit`)
- `operation_class: "read"` → allows only `read`

**Default:** If `operation_class` is absent, default is `read`.

### 2.3 Payload Type

```
application/vnd.assay.mandate+json;v=1
```

This value MUST be used in `signature.payload_type` for type confusion prevention.

---

## 3. Event Schemas

### 3.1 assay.mandate.v1

CloudEvents envelope with mandate grant payload.

**CloudEvents requirements (MUST):**

| Field | Requirement |
|-------|-------------|
| `specversion` | MUST be `"1.0"` |
| `id` | MUST be present, unique per source |
| `type` | MUST be `"assay.mandate.v1"` |
| `source` | MUST be present, valid URI |
| `time` | MUST be present, RFC 3339 UTC timestamp |
| `datacontenttype` | MUST be `"application/json"` |
| `data` | MUST be JSON object (not string-encoded) |
| `subject` | MAY be present for tool_call_id correlation |

> **v1.0.2:** Explicit required attributes list aligns with CloudEvents v1.0 §2.1. The `subject` attribute MAY be used as CloudEvents-native correlation alternative to `data.tool_call_id`.

```json
{
  "specversion": "1.0",
  "id": "evt_abc123",
  "type": "assay.mandate.v1",
  "source": "assay://myorg/myapp",
  "time": "2026-01-28T10:00:00Z",
  "datacontenttype": "application/json",
  "data": {
    "mandate_id": "sha256:abc123def456...",
    "mandate_kind": "intent",

    "principal": {
      "subject": "opaque-subject-id",
      "method": "oidc",
      "display": "Alice (shopping)",
      "credential_ref": "sha256:789xyz..."
    },

    "scope": {
      "tools": ["search_*", "list_*"],
      "resources": ["/products/**", "/catalog/**"],
      "operation_class": "read",
      "max_value": null
    },

    "validity": {
      "not_before": "2026-01-28T10:00:00Z",
      "expires_at": "2026-01-28T18:00:00Z",
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
      "nonce": null,
      "traceparent": "00-0af7651916cd43dd8448eb211c80319c-b7ad6b7169203331-01"
    },

    "signature": {
      "version": 1,
      "algorithm": "ed25519",
      "payload_type": "application/vnd.assay.mandate+json;v=1",
      "content_id": "sha256:abc123def456...",
      "signed_payload_digest": "sha256:789abc012def...",
      "key_id": "sha256:signing-key-id...",
      "signature": "base64-encoded-signature...",
      "signed_at": "2026-01-28T09:55:00Z"
    }
  }
}
```

### 3.2 Field Definitions

#### 3.2.1 Root Fields

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `mandate_id` | string | Yes | Content-addressed identifier (see §2.1) |
| `mandate_kind` | enum | Yes | One of: `intent`, `transaction` |
| `principal` | object | Yes | Who granted the mandate |
| `scope` | object | Yes | What the mandate authorizes |
| `validity` | object | Yes | When the mandate is valid |
| `constraints` | object | Yes | Usage limits |
| `context` | object | Yes | Binding context for replay prevention |
| `signature` | object | No | Cryptographic signature (see §4) |

#### 3.2.2 Principal Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `subject` | string | Yes | Opaque identifier (MUST NOT contain PII) |
| `method` | enum | Yes | Authentication method (see below) |
| `display` | string | No | Human-readable name (UX only, MUST NOT use for verification) |
| `credential_ref` | string | No | Hash reference to verifiable credential |

**method enum values:**

| Value | Description |
|-------|-------------|
| `oidc` | OpenID Connect (OAuth 2.0) |
| `did` | Decentralized Identifier |
| `spiffe` | SPIFFE/SPIRE workload identity |
| `local_user` | Local system user |
| `service_account` | Service-to-service |
| `api_key` | API key authentication |

**credential_ref format:**

```
"sha256:" + lowercase_hex(SHA256(credential_bytes))
```

Where `credential_bytes` is:
- For JWT VP: raw UTF-8 bytes of the compact JWT
- For JSON VP: JCS-canonicalized bytes
- v1: Opaque string, MUST be stable within organization

#### 3.2.3 Scope Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `tools` | string[] | Yes | Tool name patterns (glob syntax) |
| `resources` | string[] | No | Resource path patterns (glob syntax) |
| `operation_class` | enum | No | Highest operation class allowed (default: `read`) |
| `max_value` | object | No | Maximum transaction value |
| `transaction_ref` | string | No | Hash of cart/order intent object (for commit mandates) |

**transaction_ref (for commit mandates):**

For `operation_class: commit` mandates, `transaction_ref` provides object-level authorization binding:

```json
{
  "scope": {
    "tools": ["purchase_item"],
    "operation_class": "commit",
    "transaction_ref": "sha256:cart-content-hash-here..."
  }
}
```

Computation: `transaction_ref = "sha256:" + hex(SHA256(JCS(transaction_object)))`

Where `transaction_object` is the cart, order, or payment intent that this mandate authorizes. This prevents mandate reuse for different transactions within the validity window.

**Transaction Intent Object Schema (v1.0.2 NORMATIVE):**

For interoperability, the `transaction_object` SHOULD conform to this minimal schema:

```json
{
  "merchant": "string",           // REQUIRED: Merchant identifier
  "items": [                      // REQUIRED: Line items (order preserved)
    {
      "product_id": "string",     // REQUIRED: Product identifier
      "quantity": 1,              // REQUIRED: Integer quantity
      "unit_price": "10.00"       // OPTIONAL: Decimal string
    }
  ],
  "total": {                      // REQUIRED: Total amount
    "amount": "100.00",           // Decimal string, MUST NOT use float
    "currency": "USD"             // ISO 4217, MUST be uppercase
  },
  "created_at": "2026-01-28T10:00:00Z"  // OPTIONAL: ISO 8601 UTC
}
```

**Normalization rules for JCS hashing:**
- `amount` fields MUST be decimal strings (no floats, no trailing zeros: "10" not "10.00")
- `currency` MUST be uppercase ISO 4217
- `items` array order MUST be preserved (JCS preserves array order)
- No optional fields should be present with `null` values; omit them entirely

**Verification:** Runtime MUST verify that the actual transaction content hashes to the same value as `transaction_ref` before allowing commit tools.

**tools pattern syntax (NORMATIVE):**

Pattern matching rules (producers and verifiers MUST use identical algorithm):

| Rule | Specification |
|------|---------------|
| **Anchoring** | Pattern MUST match the **full tool name** (not substring) |
| **Case sensitivity** | Matching is **case-sensitive** |
| **`*` (single glob)** | Matches any sequence of characters **except** `.` (dot) |
| **`**` (double glob)** | Matches any sequence of characters **including** `.` (dot) |
| **Literal characters** | All non-glob characters match themselves exactly |
| **Escaping** | Use `\*` to match literal `*`; use `\\` to match literal `\` |

**Examples:**

```
search_*      → matches: search_products, search_users
              → does NOT match: search.products (dot not matched by *)
fs.read_*     → matches: fs.read_file, fs.read_dir
              → does NOT match: fs.read.file (second dot)
fs.**         → matches: fs.read_file, fs.write.nested.path
*             → matches: search, list (single-segment names only)
**            → matches: any tool name (universal wildcard)
```

**Implementation requirements (v1.0.2):**

> ⚠️ **MUST NOT use OS glob libraries.** Standard glob implementations (Python's `fnmatch`, shell glob, Go's `filepath.Match`) use different semantics for `*` (often matches `.`). Implementers MUST use the Assay Glob v1 algorithm defined above, or a conforming implementation.

Conforming implementations are available in:
- Rust: `assay_evidence::mandate::glob`
- Python: `assay.glob` (planned)

**Canonicalization:** Tool names MUST be normalized to lowercase before matching if the runtime uses case-insensitive tool names. The `tools` array in mandates SHOULD use lowercase patterns for maximum compatibility.

**max_value object:**

```json
{
  "amount": "100.00",   // Decimal as string, MUST NOT use float
  "currency": "USD"     // ISO 4217 currency code
}
```

#### 3.2.4 Validity Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `issued_at` | datetime | Yes | When mandate was created (ISO 8601 UTC) |
| `not_before` | datetime | No | Mandate valid after this time |
| `expires_at` | datetime | No | Mandate expires at this time |

**Time comparison semantics:**

- `not_before`: mandate valid if `now >= not_before`
- `expires_at`: mandate valid if `now < expires_at`
- If omitted: no constraint on that boundary

#### 3.2.5 Constraints Object

| Field | Type | Required | Default | Description |
|-------|------|----------|---------|-------------|
| `single_use` | boolean | No | `false` | Syntactic sugar for `max_uses: 1` |
| `max_uses` | integer | No | `null` | Maximum uses (`null` = unlimited) |
| `require_confirmation` | boolean | No | `false` | Require interactive confirmation |

**max_uses semantics:**

| Value | Meaning |
|-------|---------|
| `null` | Unlimited uses |
| `1` | Single use (equivalent to `single_use: true`) |
| `N` | Maximum N uses; rejected after Nth use |

#### 3.2.6 Context Object

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `audience` | string | Yes | Target application/org identifier |
| `issuer` | string | Yes | Signing authority identifier |
| `nonce` | string | No | Session binding (for interactive flows) |
| `traceparent` | string | No | W3C Trace Context for correlation |

### 3.3 assay.mandate.used.v1

Consumption receipt for usage tracking.

```json
{
  "specversion": "1.0",
  "id": "evt_use456",
  "type": "assay.mandate.used.v1",
  "source": "assay://myorg/myapp",
  "time": "2026-01-28T10:05:00Z",
  "datacontenttype": "application/json",
  "data": {
    "mandate_id": "sha256:abc123def456...",
    "use_id": "sha256:use789...",
    "tool_call_id": "tc_456",
    "consumed_at": "2026-01-28T10:05:00Z",
    "use_count": 1
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `mandate_id` | string | Yes | Reference to consumed mandate |
| `use_id` | string | Yes | Unique identifier for this use |
| `tool_call_id` | string | Yes | Tool call that consumed the mandate |
| `consumed_at` | datetime | Yes | When consumption occurred |
| `use_count` | integer | Yes | Ordinal use number (1-indexed) |

### 3.4 assay.mandate.revoked.v1

Revocation event for mandate cancellation.

```json
{
  "specversion": "1.0",
  "id": "evt_rev789",
  "type": "assay.mandate.revoked.v1",
  "source": "assay://myorg/myapp",
  "time": "2026-01-28T10:30:00Z",
  "datacontenttype": "application/json",
  "data": {
    "mandate_id": "sha256:abc123def456...",
    "revoked_at": "2026-01-28T10:30:00Z",
    "reason": "user_requested",
    "revoked_by": "opaque-subject-id"
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `mandate_id` | string | Yes | Mandate being revoked |
| `revoked_at` | datetime | Yes | Effective revocation time |
| `reason` | enum | Yes | Revocation reason |
| `revoked_by` | string | Yes | Subject who revoked |

**reason enum values:**

| Value | Description |
|-------|-------------|
| `user_requested` | User explicitly revoked |
| `admin_override` | Administrative action |
| `policy_violation` | Automated policy enforcement |
| `expired_early` | Voluntary early expiration |

**Revocation semantics:**

| Aspect | Behavior |
|--------|----------|
| Effect | Mandate MUST NOT be used after `revoked_at` |
| Retroactivity | NOT retroactive; uses before `revoked_at` remain valid |
| Ordering | Runtime: reject if `now >= revoked_at`; Lint: compare `tool.decision.time` vs `revoked_at` |

### 3.5 Event Trust Model

Mandate lifecycle events (`used`, `revoked`) are vulnerable to injection attacks without proper trust controls.

**Trust requirements (MUST):**

| Event Type | Trust Requirement |
|------------|-------------------|
| `assay.mandate.v1` | MUST be signed (as per §4) |
| `assay.mandate.used.v1` | MUST originate from trusted source (see below) |
| `assay.mandate.revoked.v1` | MUST originate from trusted source (see below) |

**Trusted source verification:**

```yaml
# In policy config
mandate_trust:
  # Trusted sources for lifecycle events
  trusted_event_sources:
    - "assay://myorg/myapp"
    - "assay://myorg/auth-service"

  # Require signed lifecycle events
  # DEFAULT (v1.0.2): true when mandate_kind=transaction OR tool ∈ commit_tools
  require_signed_lifecycle_events: auto  # "auto" | true | false
```

**v1.0.2 default behavior for `require_signed_lifecycle_events: auto`:**

| Mandate Kind | Tool Classification | Lifecycle Events |
|--------------|---------------------|------------------|
| `intent` | read tools | Source check only |
| `intent` | write tools | Source check only |
| `transaction` | any tool | **MUST be signed** |
| any | commit tools | **MUST be signed** |

This default acknowledges that lifecycle events for high-value operations (transactions, commits) are high-risk injection targets.

**Verification rules:**

1. `event.source` MUST be in `trusted_event_sources` list
2. If signatures required (see table above):
   - `used` and `revoked` events MUST include a `signature` object
   - Signature verification follows same algorithm as mandates (see §4)
   - Signature `payload_type` MUST be `application/vnd.assay.mandate.used+json;v=1` or `application/vnd.assay.mandate.revoked+json;v=1`
3. Evidence bundles MUST be treated as tamper-evident containers; events from untrusted sources MUST be rejected at ingest

**Adversarial model considerations:**

Without these controls, attackers could:
- Inject fake `revoked` events → DoS (mandate appears invalid)
- Inject fake `used` events → Force `max_uses` exceeded
- Replay old lifecycle events → State confusion

**Optional signature for lifecycle events:**

For high-risk deployments (commerce, financial), add `signature` to `used`/`revoked` events:

```json
{
  "type": "assay.mandate.used.v1",
  "data": {
    "mandate_id": "sha256:...",
    "use_id": "sha256:...",
    "tool_call_id": "tc_456",
    "consumed_at": "2026-01-28T10:05:00Z",
    "use_count": 1,
    "signature": {
      "version": 1,
      "algorithm": "ed25519",
      "payload_type": "application/vnd.assay.mandate.used+json;v=1",
      "payload_digest": "sha256:...",
      "key_id": "sha256:...",
      "signature": "base64...",
      "signed_at": "2026-01-28T10:05:00Z"
    }
  }
}
```

### 3.6 Tool Decision Extension

Extended `assay.tool.decision` with mandate linkage.

```json
{
  "type": "assay.tool.decision",
  "data": {
    "tool": "purchase_item",
    "decision": "allow",
    "reason_code": "P_MANDATE_VALID",
    "args_schema_hash": "sha256:...",
    "tool_call_id": "tc_456",
    "mandate_id": "sha256:abc123def456...",
    "mandate_scope_match": true,
    "mandate_kind_match": true
  }
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `mandate_id` | string | Conditional | Mandate authorizing this decision |
| `mandate_scope_match` | boolean | No | Whether tool matched mandate scope |
| `mandate_kind_match` | boolean | No | Whether mandate kind allows operation class |

---

## 4. Signing Process

Mandate signing follows the same DSSE-compatible process as [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md).

### 4.1 Signature Object

```json
{
  "version": 1,
  "algorithm": "ed25519",
  "payload_type": "application/vnd.assay.mandate+json;v=1",
  "content_id": "sha256:abc123...",
  "signed_payload_digest": "sha256:def789...",
  "key_id": "sha256:signing-key-id...",
  "signature": "base64-encoded-signature...",
  "signed_at": "2026-01-28T09:55:00Z"
}
```

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | integer | Yes | Schema version. MUST be `1` |
| `algorithm` | string | Yes | MUST be `"ed25519"` for v1 |
| `payload_type` | string | Yes | MUST be `"application/vnd.assay.mandate+json;v=1"` |
| `content_id` | string | Yes | MUST equal `mandate_id` (content-addressed identifier) |
| `signed_payload_digest` | string | Yes | SHA256 of signed payload bytes (DSSE standard) |
| `key_id` | string | Yes | SHA-256 of SPKI public key |
| `signature` | string | Yes | Base64-encoded Ed25519 signature |
| `signed_at` | datetime | Yes | Signing timestamp (metadata only) |

> **v1.0.2 change:** Renamed `payload_digest` to `content_id` and added `signed_payload_digest` for DSSE alignment. This prevents implementer confusion where "payload_digest" is expected to be the digest of the signed payload.

### 4.2 Signing Algorithm

```
1. Build hashable_content = data object WITHOUT {mandate_id, signature}
2. Compute canonical_for_id = JCS(hashable_content)
3. Compute mandate_id = "sha256:" + hex(SHA256(canonical_for_id))
4. Build signable_content = hashable_content + {mandate_id: mandate_id}
5. Compute canonical_for_sig = JCS(signable_content)
6. Compute signed_payload_digest = "sha256:" + hex(SHA256(canonical_for_sig))
7. Compute PAE = DSSEv1_PAE(payload_type, canonical_for_sig)
8. Sign: signature_bytes = ed25519_sign(private_key, PAE)
9. Build signature object:
   - content_id = mandate_id
   - signed_payload_digest = signed_payload_digest (from step 6)
   - signature = base64_encode_with_padding(signature_bytes)
10. Build final_content = signable_content + {signature: signature_object}
11. Emit CloudEvents envelope with data = final_content
```

**Important:**
- Steps 1-3 compute the content-addressed ID from content WITHOUT mandate_id (avoiding circularity)
- Steps 4-6 compute the signed payload digest from content WITH mandate_id
- Steps 7-8 sign using DSSE PAE encoding
- `content_id` = identifier for lookups/references
- `signed_payload_digest` = standard DSSE payload digest for verification

### 4.3 PAE Encoding (DSSE)

```
PAE(type, payload) =
    "DSSEv1" + SP +
    LEN(type) + SP + type + SP +
    LEN(payload) + SP + payload

Where:
    SP = 0x20 (space character)
    LEN(s) = ASCII decimal byte length, no leading zeros
```

---

## 5. Verification Process

### 5.1 Verification Algorithm

```
1. Parse event, extract data as mandate_content
2. Extract sig = mandate_content.signature
3. If sig is missing:
   a. If config.require_signed: FAIL (UNSIGNED)
   b. Else: PASS (unsigned allowed)
4. Validate sig.version == 1
5. Validate sig.algorithm == "ed25519"
6. Validate sig.payload_type == "application/vnd.assay.mandate+json;v=1"

// Verify content_id == mandate_id (content-addressed)
7. Extract claimed_id = mandate_content.mandate_id
8. Validate claimed_id == sig.content_id
9. Build hashable = mandate_content WITHOUT {mandate_id, signature}
10. Compute canonical_for_id = JCS(hashable)
11. Compute computed_id = "sha256:" + hex(SHA256(canonical_for_id))
12. Validate computed_id == claimed_id  // CRITICAL: proves ID is content-addressed

// Verify signed_payload_digest (DSSE alignment)
13. Build signable = mandate_content WITHOUT {signature} (but WITH mandate_id)
14. Compute canonical_for_sig = JCS(signable)
15. Compute computed_signed_digest = "sha256:" + hex(SHA256(canonical_for_sig))
16. Validate computed_signed_digest == sig.signed_payload_digest

// Verify signature
17. Compute PAE = DSSEv1_PAE(sig.payload_type, canonical_for_sig)
18. Obtain public_key by sig.key_id from trust policy
19. Verify ed25519_verify(public_key, PAE, base64_decode(sig.signature))
20. If invalid: FAIL (INVALID_SIGNATURE)

// Additional checks
21. Check context binding (see §5.2)
22. Check validity window with clock skew (see §5.3)
23. Check revocation status (see §5.4)
24. PASS
```

**Note:** Steps 7-12 verify content addressing; steps 13-16 verify signed payload digest (DSSE standard). Both MUST pass.

### 5.2 Context Binding Verification

```
1. Load config.expected_audience and config.trusted_issuers
2. Validate mandate.context.audience == config.expected_audience
3. Validate mandate.context.issuer IN config.trusted_issuers
4. If nonce present: verify against session store (implementation-specific)
5. If any check fails: FAIL (CONTEXT_MISMATCH)
```

### 5.3 Validity Window Verification

**Runtime (wall clock):**

```rust
fn check_validity(mandate: &Mandate, now: DateTime<Utc>) -> Result<()> {
    if let Some(nb) = mandate.validity.not_before {
        if now < nb { return Err(NotYetValid); }
    }
    if let Some(exp) = mandate.validity.expires_at {
        if now >= exp { return Err(Expired); }
    }
    Ok(())
}
```

**Lint (event time):**

```rust
fn check_validity_lint(mandate: &Mandate, event_time: DateTime<Utc>) -> Result<()> {
    // Same logic, but using event.time instead of Utc::now()
}
```

### 5.4 Revocation Check

```
1. Query store for revocation events with matching mandate_id
2. If revocation exists:
   a. Runtime: reject if now >= revocation.revoked_at
   b. Lint: reject if tool_decision.time >= revocation.revoked_at
```

### 5.5 Exit Codes

| Code | Name | Description |
|------|------|-------------|
| 0 | SUCCESS | Valid signature, trusted key, valid context |
| 1 | ERROR | I/O error, malformed JSON |
| 2 | UNSIGNED | No signature when required |
| 3 | UNTRUSTED | Valid signature, untrusted key |
| 4 | INVALID_SIGNATURE | Bad signature, digest mismatch |
| 5 | CONTEXT_MISMATCH | Audience/issuer verification failed |
| 6 | EXPIRED | Mandate outside validity window |
| 7 | REVOKED | Mandate has been revoked |
| 8 | MAX_USES_EXCEEDED | Consumption limit reached |

---

## 6. Trust Policy

### 6.1 Configuration Format

```yaml
# assay.yaml or policy.yaml
mandate_trust:
  # Require all mandates to be signed
  require_signed: true

  # Expected audience (must match mandate.context.audience)
  # Format: {org}/{app} or {org}/{app}/{env}
  expected_audience: "myorg/myapp"

  # Trusted issuers (mandate.context.issuer must be in list)
  # Comparison is exact string match
  trusted_issuers:
    - "auth.myorg.com"
    - "idp.partner.com"

  # Trusted signing key IDs
  trusted_key_ids:
    - "sha256:abc123..."  # Production key
    - "sha256:def456..."  # CI key

  # Allow embedded public key (development only)
  allow_embedded_key: false

  # Clock skew tolerance in seconds (default: 30)
  clock_skew_tolerance_seconds: 30

  # Trusted sources for lifecycle events (used, revoked)
  trusted_event_sources:
    - "assay://myorg/myapp"
    - "assay://myorg/auth-service"

  # Require signed lifecycle events (recommended for high-risk)
  require_signed_lifecycle_events: false

  # Tool classification for operation_class enforcement
  # Patterns use same glob syntax as mandate scope
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

### 6.2 Operation Class Enforcement

To determine if a tool requires `transaction` mandate:

```
1. Match tool name against commit_tools patterns
2. If match: require mandate_kind == "transaction"
3. Match tool name against write_tools patterns
4. If match: require mandate_kind in ["intent", "transaction"]
5. Else: require any valid mandate
```

---

## 7. Single-Use Enforcement

### 7.1 Runtime Enforcement

```rust
async fn consume_mandate(
    mandate_id: &str,
    tool_call_id: &str,
    store: &Store
) -> Result<u32> {
    // Atomic increment-and-check
    let use_count = store.increment_use_count(mandate_id).await?;

    let mandate = store.get_mandate(mandate_id).await?;

    // Check single_use constraint
    if mandate.constraints.single_use && use_count > 1 {
        return Err(MandateError::AlreadyUsed);
    }

    // Check max_uses constraint
    if let Some(max) = mandate.constraints.max_uses {
        if use_count > max {
            return Err(MandateError::MaxUsesExceeded);
        }
    }

    // Emit receipt event
    emit_mandate_use_event(mandate_id, tool_call_id, use_count);

    Ok(use_count)
}
```

### 7.2 Lint Enforcement

```
1. Collect all assay.mandate.used.v1 events for mandate_id
2. Count unique use_id values
3. If mandate.constraints.single_use && count > 1: FAIL
4. If mandate.constraints.max_uses && count > max_uses: FAIL
```

---

## 8. Pack Rules

### 8.1 mandate-baseline.yaml

| Rule ID | Check | Severity | Scope | Engine Support |
|---------|-------|----------|-------|----------------|
| MANDATE-001 | `decision=allow` for `commit` tools MUST have `mandate_id` | error | commit tools only | v1 (conditional) |
| MANDATE-002 | `mandate_id` MUST reference existing `assay.mandate.v1` | error | all | v1.1 (reference_exists) |
| MANDATE-003 | Tool decision time within mandate validity window | error | all | v1.1 (temporal_range) |
| MANDATE-004 | `single_use`/`max_uses` mandate has valid receipt count | error | all | v1.1 (use_count_valid) |
| MANDATE-005 | `commit` tools require `mandate_kind=transaction` | warning | commit tools | v1.1 (mandate_kind_check) |

**Engine capability requirements:**

| Check Type | Minimum Engine Version | Status |
|------------|------------------------|--------|
| `conditional` | v1.0 | Implemented |
| `json_path_exists` | v1.0 | Implemented |
| `reference_exists` | v1.1 | Planned |
| `temporal_range` | v1.1 | Planned |
| `use_count_valid` | v1.1 | Planned |
| `mandate_kind_check` | v1.1 | Planned |

**Note:** Rules requiring v1.1 check types will be skipped with a warning on v1.0 engines. The `mandate-baseline.yaml` pack will be published when engine v1.1 is available.

**Note on MANDATE-001 scope:** To prevent false positives in discovery flows, this rule only applies to tools classified as `commit` (per `mandate_trust.commit_tools`). Read-only discovery operations do not require mandate linkage.

### 8.2 Rule Definitions

```yaml
rules:
  - id: MANDATE-001
    description: "Commit tool decisions must have mandate authorization"
    check:
      type: conditional
      condition:
        all:
          - path: "/data/decision"
            equals: "allow"
          - path: "/data/tool"
            matches_any: "${mandate_trust.commit_tools}"
      then:
        type: json_path_exists
        paths: ["/data/mandate_id"]
    event_types: ["assay.tool.decision"]
    severity: error

  - id: MANDATE-002
    description: "mandate_id must reference existing mandate"
    check:
      type: reference_exists
      source_path: "/data/mandate_id"
      target_event_type: "assay.mandate.v1"
      target_path: "/data/mandate_id"
    event_types: ["assay.tool.decision"]
    severity: error

  - id: MANDATE-003
    description: "Tool decision must be within mandate validity window"
    check:
      type: temporal_range
      event_time_path: "/time"
      mandate_ref_path: "/data/mandate_id"
      not_before_path: "/data/validity/not_before"
      expires_at_path: "/data/validity/expires_at"
    event_types: ["assay.tool.decision"]
    severity: error

  - id: MANDATE-004
    description: "Single-use mandate must have exactly one use receipt"
    check:
      type: use_count_valid
      mandate_path: "/data/mandate_id"
      single_use_path: "/data/constraints/single_use"
      max_uses_path: "/data/constraints/max_uses"
    event_types: ["assay.mandate.v1"]
    severity: error

  - id: MANDATE-005
    description: "Commit tools require transaction mandate"
    check:
      type: conditional
      condition:
        all:
          - path: "/data/tool"
            matches_any: "${mandate_trust.commit_tools}"
          - path: "/data/decision"
            equals: "allow"
      then:
        type: mandate_kind_check
        mandate_ref_path: "/data/mandate_id"
        required_kind: "transaction"
    event_types: ["assay.tool.decision"]
    severity: warning
```

---

## 9. Examples

### 9.1 Intent Mandate (Standing Authority)

```json
{
  "specversion": "1.0",
  "id": "evt_intent_001",
  "type": "assay.mandate.v1",
  "source": "assay://acme-corp/shopping-agent",
  "time": "2026-01-28T09:00:00Z",
  "data": {
    "mandate_id": "sha256:a1b2c3d4e5f6789012345678901234567890123456789012345678901234abcd",
    "mandate_kind": "intent",
    "principal": {
      "subject": "usr_K7xM2nP9qR4s",
      "method": "oidc",
      "display": "Alice (shopping)"
    },
    "scope": {
      "tools": ["search_*", "list_*", "get_product_*"],
      "resources": ["/products/**", "/reviews/**"],
      "operation_class": "read",
      "max_value": null
    },
    "validity": {
      "not_before": "2026-01-28T09:00:00Z",
      "expires_at": "2026-01-28T17:00:00Z",
      "issued_at": "2026-01-28T08:55:00Z"
    },
    "constraints": {
      "single_use": false,
      "max_uses": null,
      "require_confirmation": false
    },
    "context": {
      "audience": "acme-corp/shopping-agent",
      "issuer": "auth.acme-corp.com",
      "nonce": null,
      "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
    },
    "signature": {
      "version": 1,
      "algorithm": "ed25519",
      "payload_type": "application/vnd.assay.mandate+json;v=1",
      "payload_digest": "sha256:a1b2c3d4e5f6789012345678901234567890123456789012345678901234abcd",
      "key_id": "sha256:prod-signing-key-fingerprint-here-64-hex-chars-total-ok",
      "signature": "MEUCIQC...",
      "signed_at": "2026-01-28T08:55:00Z"
    }
  }
}
```

### 9.2 Transaction Mandate (Final Authorization)

```json
{
  "specversion": "1.0",
  "id": "evt_txn_001",
  "type": "assay.mandate.v1",
  "source": "assay://acme-corp/shopping-agent",
  "time": "2026-01-28T10:30:00Z",
  "data": {
    "mandate_id": "sha256:f1e2d3c4b5a6789012345678901234567890123456789012345678901234wxyz",
    "mandate_kind": "transaction",
    "principal": {
      "subject": "usr_K7xM2nP9qR4s",
      "method": "oidc",
      "credential_ref": "sha256:vp-hash-from-interactive-confirmation"
    },
    "scope": {
      "tools": ["purchase_item"],
      "resources": ["/cart/current"],
      "operation_class": "commit",
      "max_value": {
        "amount": "99.99",
        "currency": "USD"
      }
    },
    "validity": {
      "not_before": "2026-01-28T10:30:00Z",
      "expires_at": "2026-01-28T10:35:00Z",
      "issued_at": "2026-01-28T10:30:00Z"
    },
    "constraints": {
      "single_use": true,
      "max_uses": 1,
      "require_confirmation": true
    },
    "context": {
      "audience": "acme-corp/shopping-agent",
      "issuer": "auth.acme-corp.com",
      "nonce": "confirm_session_xyz789",
      "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-b7ad6b7169203331-01"
    },
    "signature": {
      "version": 1,
      "algorithm": "ed25519",
      "payload_type": "application/vnd.assay.mandate+json;v=1",
      "payload_digest": "sha256:f1e2d3c4b5a6789012345678901234567890123456789012345678901234wxyz",
      "key_id": "sha256:prod-signing-key-fingerprint-here-64-hex-chars-total-ok",
      "signature": "MEYCIQDy...",
      "signed_at": "2026-01-28T10:30:00Z"
    }
  }
}
```

### 9.3 Tool Decision with Mandate

```json
{
  "specversion": "1.0",
  "id": "evt_decision_001",
  "type": "assay.tool.decision",
  "source": "assay://acme-corp/shopping-agent",
  "time": "2026-01-28T10:31:00Z",
  "data": {
    "tool": "purchase_item",
    "decision": "allow",
    "reason_code": "P_MANDATE_VALID",
    "tool_call_id": "tc_purchase_001",
    "mandate_id": "sha256:f1e2d3c4b5a6789012345678901234567890123456789012345678901234wxyz",
    "mandate_scope_match": true,
    "mandate_kind_match": true
  }
}
```

### 9.4 Consumption Receipt

```json
{
  "specversion": "1.0",
  "id": "evt_use_001",
  "type": "assay.mandate.used.v1",
  "source": "assay://acme-corp/shopping-agent",
  "time": "2026-01-28T10:31:00Z",
  "data": {
    "mandate_id": "sha256:f1e2d3c4b5a6789012345678901234567890123456789012345678901234wxyz",
    "use_id": "sha256:use_abc123",
    "tool_call_id": "tc_purchase_001",
    "consumed_at": "2026-01-28T10:31:00Z",
    "use_count": 1
  }
}
```

---

## 10. Security Considerations

### 10.1 Principal Privacy

- `subject` MUST be opaque; MUST NOT contain email, name, or other PII
- `display` is for UX only; verifiers MUST NOT use it for trust decisions
- `display` SHOULD be absent in exported audit bundles unless explicitly needed
- `display` MUST be redacted when sharing evidence with third parties
- Use organizational pseudonyms or hashed identifiers (e.g., `usr_K7xM2nP9qR4s`)

**Anti-pattern examples (MUST NOT):**
```json
// BAD - contains PII
"display": "user@example.com"
"display": "John Smith"
"display": "+1-555-123-4567"

// GOOD - no PII
"display": "Alice (shopping)"
"display": "user-1234"
"display": null
```

### 10.2 Replay Prevention

- `context.audience` MUST be a stable identifier of application+tenant (e.g., `org/app` or `org/app/env`)
- `context.issuer` MUST map to a trust policy entry (string equality, no normalization)
- Transaction mandates SHOULD use `nonce` for session binding
- Standing mandates rely on `audience` + `issuer` + short validity

**Nonce requirements (for transaction mandates):**

| Requirement | Specification |
|-------------|---------------|
| Presence | SHOULD be present for `mandate_kind: transaction` |
| Entropy | Minimum 128 bits (e.g., 22+ Base64 characters) |
| Uniqueness | MUST be unique per session/confirmation flow |
| Storage | Runtime MUST track used nonces to prevent replay |

### 10.3 Clock Skew

Clock skew tolerance is configurable and MUST be auditable.

**Policy configuration:**

```yaml
mandate_trust:
  # Clock skew tolerance in seconds (default: 30)
  clock_skew_tolerance_seconds: 30
```

**Behavior:**

- Runtime validity check: `now - skew <= not_before` and `now + skew < expires_at`
- Lint mode uses CloudEvents `time` field, not wall clock
- `not_before` may be slightly in the future to account for distribution

**Audit reporting:**

Lint reports MUST include skew information when tolerance is applied:

```json
{
  "rule": "MANDATE-003",
  "result": "pass",
  "details": {
    "validity_check": "passed_with_skew",
    "skew_applied_seconds": 27,
    "configured_tolerance_seconds": 30
  }
}
```

### 10.4 Context Binding (Normative)

**audience verification:**

```
MUST: mandate.context.audience == config.expected_audience
```

`expected_audience` SHOULD follow pattern: `{org}/{app}` or `{org}/{app}/{env}`

**issuer verification:**

```
MUST: mandate.context.issuer IN config.trusted_issuers
```

Comparison is exact string match; no URL normalization is performed.

**traceparent binding:**

If present, `traceparent` SHOULD match the W3C Trace Context of the current request. This enables correlation in distributed tracing systems but is NOT used for security decisions.

### 10.5 Key Management

- Same key management as tool signing ([SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md))
- Private keys: mode `0600`, not in version control
- Rotate keys periodically; old keys remain trusted for verification

### 10.6 Base64 Encoding

All Base64 values in this specification (signatures, hashes) MUST use:
- Standard Base64 alphabet (RFC 4648 §4)
- WITH padding (`=` characters)

Parsers MAY accept Base64 without padding for compatibility, but producers MUST include padding.

---

## 11. Conformance Test Vectors (v1.0.2)

Implementations MUST pass all test vectors in this section.

### 11.1 Glob Matching Vectors

| Pattern | Input | Expected | Reason |
|---------|-------|----------|--------|
| `search_*` | `search_products` | ✓ match | `*` matches `products` |
| `search_*` | `search_users` | ✓ match | `*` matches `users` |
| `search_*` | `search_` | ✓ match | `*` matches empty string |
| `search_*` | `search.products` | ✗ no match | `*` stops at `.` |
| `search_*` | `search` | ✗ no match | Missing `_` |
| `search_*` | `Search_products` | ✗ no match | Case-sensitive |
| `fs.read_*` | `fs.read_file` | ✓ match | Literal `.` matches |
| `fs.read_*` | `fs.read.file` | ✗ no match | `*` stops at second `.` |
| `fs.**` | `fs.read_file` | ✓ match | `**` matches any |
| `fs.**` | `fs.write.nested.path` | ✓ match | `**` matches `.` |
| `*` | `search` | ✓ match | `*` matches single segment |
| `*` | `ns.tool` | ✗ no match | `*` stops at `.` |
| `**` | `anything.at.all` | ✓ match | Universal wildcard |
| `file\*name` | `file*name` | ✓ match | Escaped `*` |
| `path\\to` | `path\to` | ✓ match | Escaped `\` |

### 11.2 JCS Canonicalization Vector

**Input (JSON with unordered keys):**

```json
{
  "mandate_kind": "intent",
  "context": {"issuer": "auth.myorg.com", "audience": "myorg/app"},
  "principal": {"method": "oidc", "subject": "user-123"},
  "validity": {"issued_at": "2026-01-28T10:00:00Z"},
  "scope": {"tools": ["search_*"], "operation_class": "read"},
  "constraints": {}
}
```

**Expected JCS output (single line, sorted keys):**

```
{"constraints":{},"context":{"audience":"myorg/app","issuer":"auth.myorg.com"},"mandate_kind":"intent","principal":{"method":"oidc","subject":"user-123"},"scope":{"operation_class":"read","tools":["search_*"]},"validity":{"issued_at":"2026-01-28T10:00:00Z"}}
```

**Expected mandate_id:**

```
sha256:e8f7a6b5c4d3e2f1a0b9c8d7e6f5a4b3c2d1e0f9a8b7c6d5e4f3a2b1c0d9e8f7
```

> Note: Actual hash value depends on exact JCS output bytes. Implementations MUST produce identical bytes to produce identical hashes.

### 11.3 Time Validity Vectors

| now (event time) | not_before | expires_at | skew_seconds | Expected |
|------------------|------------|------------|--------------|----------|
| 10:00:00 | 09:00:00 | 11:00:00 | 0 | ✓ valid |
| 10:00:00 | 10:00:30 | 11:00:00 | 30 | ✓ valid (skew) |
| 10:00:00 | 10:01:00 | 11:00:00 | 30 | ✗ not_yet_valid |
| 10:00:00 | 09:00:00 | 10:00:00 | 0 | ✗ expired (exclusive) |
| 10:00:00 | 09:00:00 | 09:59:30 | 30 | ✗ expired |
| 10:00:00 | null | 11:00:00 | 0 | ✓ valid |
| 10:00:00 | 09:00:00 | null | 0 | ✓ valid |

### 11.4 use_id Generation (NORMATIVE v1.0.2)

`use_id` MUST be content-addressed:

```
use_id = "sha256:" + hex(SHA256(JCS({
  "mandate_id": "<mandate_id>",
  "tool_call_id": "<tool_call_id>",
  "use_count": <use_count>
})))
```

This ensures:
- Deterministic generation (same inputs → same ID)
- Uniqueness (different tool_call_id or use_count → different ID)
- Verifiability (third parties can recompute)

### 11.5 JSON Parsing Requirements (NORMATIVE)

Parsers MUST reject JSON with:
- **Duplicate keys**: `{"a": 1, "a": 2}` MUST be rejected
- **Trailing data**: `{"a": 1}garbage` MUST be rejected
- **Comments**: `{"a": 1 /* comment */}` MUST be rejected (not valid JSON)

Rationale: Canonicalization attacks exploit parser differences in duplicate key handling.

---

## 12. Future Extensions (v2)

| Feature | Description |
|---------|-------------|
| OpenID4VP binding | Normative VP canonicalization per credential format |
| Sigstore keyless | Fulcio certificates + Rekor transparency log |
| Delegation chains | Mandate-to-mandate delegation with proof chain |
| Transaction details | Cart hash, line items for commerce verification |
| Multi-signature | Require N-of-M signatures for high-value mandates |

---

## 12. References

- [ADR-017: Mandate/Intent Evidence](./ADR-017-Mandate-Evidence.md) - Design decision
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md) - Signing format (reused)
- [RFC 8785: JSON Canonicalization Scheme](https://www.rfc-editor.org/rfc/rfc8785) - JCS
- [DSSE: Dead Simple Signing Envelope](https://github.com/secure-systems-lab/dsse) - PAE format
- [CloudEvents v1.0](https://cloudevents.io/) - Event envelope
- [AP2 Protocol](https://agentpaymentsprotocol.info/specification/) - Agent payments
- [OpenID4VP](https://openid.net/specs/openid-4-verifiable-presentations-1_0.html) - Verifiable presentations
- [W3C Trace Context](https://www.w3.org/TR/trace-context/) - Distributed tracing
