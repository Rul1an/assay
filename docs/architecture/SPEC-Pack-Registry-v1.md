# SPEC-Pack-Registry-v1

**Version:** 1.0.5
**Status:** Draft
**Date:** 2026-02-06
**Related:** [ADR-016](./ADR-016-Pack-Taxonomy.md), [ADR-021](./ADR-021-Local-Pack-Discovery.md), [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)

## Abstract

This specification defines the pack registry protocol for resolving and fetching compliance packs
from remote sources. It enables enterprise pack distribution without including commercial content
in the open source repository.

---

## 1. Scope

### 1.1 In Scope

- Pack resolution order (normative order in [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md#pack-resolution-normative): path → built-in → local config dir → registry → BYOS)
- Registry HTTP API contract
- Authentication (OIDC token exchange, static token)
- Integrity verification (digest + signature)
- Pack canonicalization (strict YAML subset)
- Key trust model
- Caching behavior
- Lockfile format

### 1.2 Out of Scope

- Pack content/rules (see SPEC-Pack-Engine-v1)
- Registry hosting implementation
- Billing/licensing enforcement

### 1.3 Changelog

| Version | Changes |
|---------|---------|
| 1.0.5 | **Interop polish:** Content-Digest defined over decoded body (after content-coding); `X-Pack-Policy: commercial|open` header for signature requirement (client classifies from header, not body); canonical bytes = UTF-8, no BOM, no trailing newline; trust roots default = union, override only via explicit config; keys manifest payloadType MUST `application/vnd.assay.registry.keys.v1+json`, payload = raw JSON; pack_name grammar link to Engine spec; commercial schema evolution (additive = breaking unless schema version); BYOS `x-assay-sig` spelling; OCI = additional resolver class; media type `application/x-yaml` canonical; cache metadata `policy` |
| 1.0.4 | **Normative and consistency fixes:** §2 informative; fail-fast (no match vs matched-but-failed); commercial signature required + sidecar 404; ETag/Content-Digest; trust hierarchy; keys manifest payloadType; cache path registry/namespace; duplicate-key reject; path_ref/BYOS pin; 410 CI; lockfile v2 Phase 1; §6.3.3 + sidecar path |
| 1.0.3 | **Sidecar signature endpoint** (§6.3.3) to avoid header size limits; detached DSSE envelope; signature now separate from pack body; backward-compatible with in-header signatures |
| 1.0.2 | OIDC token exchange endpoint, DSSE envelope format, Content-Digest (RFC 9530), Vary header, key trust manifest, number policy, HEAD endpoint, cache integrity verification, lockfile extensions, 410 handling |
| 1.0.1 | Add signature verification (MUST for commercial), strict canonicalization, lockfile, ETag/304, pagination, rate limits, OCI future track |
| 1.0.0 | Initial specification |

---

## 2. Pack Resolution Order (Informative Summary)

The **normative** pack resolution order is defined in [SPEC-Pack-Engine-v1 § Pack Resolution (Normative)](./SPEC-Pack-Engine-v1.md#pack-resolution-normative). This section is an informative summary only.

1. **Path** — Existing file or directory (if dir: load `<dir>/pack.yaml`).
2. **Built-in** — By name (e.g. `eu-ai-act-baseline`); built-in wins over local config dir.
3. **Local pack directory** — Config dir (`~/.config/assay/packs` / `%APPDATA%\assay\packs`); `{name}.yaml` or `{name}/pack.yaml`.
4. **Registry** — `name@version` or pinned `name@version#sha256:...` (this SPEC).
5. **BYOS** — `s3://`, `gs://`, `az://`, etc.
6. **NotFound** — Error with suggestion.

**Resolution semantics (NORMATIVE; see SPEC-Pack-Engine-v1 for full wording):**

- **No match** — If the reference does not match the current step (e.g. not a path, not a built-in name), the resolver MUST continue to the next step. No error at this step.
- **Matched but failed** — If a step *does* match (e.g. registry ref, fetch succeeded) but verification fails (digest mismatch, signature missing/invalid for commercial, 410 revoked without opt-in), the client MUST fail immediately with a clear error. No fallback to a later step.
- Example: `name@version#sha256:...` resolved via registry → fetch OK but digest mismatch → hard error (do not try BYOS). Example: path exists but file unreadable → error at path step (do not continue).

---

## 3. Pack Reference Format

```
pack_ref := path_ref | bundled_name | registry_ref | pinned_ref | byos_ref

path_ref      := filesystem path (file or directory)
                 # File: .yaml/.yml or arbitrary; directory: must contain pack.yaml (per SPEC-Pack-Engine-v1)
                 # May be relative (./packs/foo.yaml, ../bar.yaml) or absolute (/path/to/pack.yaml, C:\...)
bundled_name  := pack_name    # pack_name as in SPEC-Pack-Engine-v1 (lowercase letters, digits, hyphens only; no underscores/dots)
registry_ref  := pack_name "@" version        # e.g., "eu-ai-act-pro@1.2.0"
pinned_ref    := pack_name "@" version "#" digest  # digest = canonical sha256 (same as X-Pack-Digest / lockfile)
byos_ref      := scheme "://" path [ "#" digest ]   # e.g., "s3://bucket/packs/custom.yaml" or "...custom.yaml#sha256:..."
```

**Path references:** Resolution of path refs (file vs directory, containment) is normative in [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md#pack-resolution-normative). **Pack name** (`pack_name`) grammar is defined in [SPEC-Pack-Engine-v1 § Pack name grammar](./SPEC-Pack-Engine-v1.md#pack-name-grammar-normative) and used consistently for bundled names, local config dir names, and registry names; do not define a different grammar here.

**Version requirement**: Registry refs MUST include version. `@latest` is NOT supported
for reproducibility.

**Pinned refs (RECOMMENDED for CI)**: Include digest for double-verification:

```bash
# Pinned ref with digest
assay evidence lint --pack "eu-ai-act-pro@1.2.0#sha256:abc123..." bundle.tar.gz
```

When digest is specified, CLI MUST verify fetched content matches before use.

---

## 4. Registry API Contract

### 4.1 Base URL

```
Default: https://registry.getassay.dev/v1
Override: ASSAY_REGISTRY_URL environment variable
```

### 4.2 Namespaces

Packs MAY be namespaced for multi-tenant access control:

```
/packs/{name}/{version}                    # Global namespace
/orgs/{org}/packs/{name}/{version}         # Organization namespace
```

**Authorization**: Organization packs require membership in the org.

### 4.3 Endpoints

#### GET /packs/{name}/{version}

Fetch pack content.

**Request:**

```http
GET /packs/eu-ai-act-pro/1.2.0 HTTP/1.1
Host: registry.getassay.dev
Authorization: Bearer <token>
Accept: application/x-yaml
Accept-Encoding: gzip
If-None-Match: "sha256:abc123..."
```

**Response (200 OK):**

```http
HTTP/1.1 200 OK
Content-Type: application/x-yaml
ETag: "sha256:abc123..."
Content-Digest: sha-256=:base64digest...:
X-Pack-Digest: sha256:abc123...
X-Pack-Signature: <base64-encoded-DSSE-envelope>
X-Pack-Key-Id: sha256:def456...
X-Pack-Policy: commercial
X-Pack-License: LicenseRef-Assay-Enterprise-1.0
Cache-Control: private, max-age=86400
Vary: Authorization, Accept-Encoding

name: eu-ai-act-pro
version: "1.2.0"
kind: compliance
...
```

**Response (304 Not Modified):**

When `If-None-Match` matches current digest:

```http
HTTP/1.1 304 Not Modified
ETag: "sha256:abc123..."
```

**Response Headers:**

| Header | Required | Description |
|--------|----------|-------------|
| `ETag` | MUST | Strong ETag; value MUST equal `X-Pack-Digest` (canonical digest). Used for conditional requests; stable across re-formatting. |
| `Content-Digest` | MUST | RFC 9530 digest of the HTTP message body **after content-coding is decoded** (i.e. the bytes presented to the application). Computed over the decoded body bytes, not over the raw wire bytes. Server MUST send when body is present. Enables clients to verify transport integrity without gzip-variant issues. |
| `X-Pack-Digest` | MUST | SHA256 digest of **canonical** content (strict YAML parse → JSON → JCS). This is the pack integrity digest used in lockfile, pins, and verification. |
| `X-Pack-Signature` | OPTIONAL | Base64-encoded DSSE envelope (see §6.3). For packs >4KB, use sidecar endpoint §6.3.3 |
| `X-Pack-Signature-Endpoint` | SHOULD (if signed) | Relative path to signature sidecar: `/packs/{name}/{version}.sig` (same path as GET sidecar) |
| `X-Pack-Key-Id` | MUST (if signed) | SHA256 of SPKI public key |
| `X-Pack-Policy` | MUST | `commercial` or `open`. Drives signature requirement: `commercial` = client MUST verify signature (missing/invalid = fail); `open` = signature optional. Client MUST NOT rely on pack body (e.g. license in YAML) to decide; use this header. |
| `X-Pack-License` | MUST | SPDX identifier (use `LicenseRef-*` for custom) |
| `Cache-Control` | MUST | `private` for authenticated, `public` for open |
| `Vary` | MUST | `Authorization, Accept-Encoding`. If the server ever serves multiple representations (e.g. different Accept), add `Vary: Accept`; for v1 only YAML is served, so Accept is informational and ETag is stable. |

**Media type:** Pack response body uses `Content-Type: application/x-yaml` as the canonical media type in this SPEC. Clients and servers SHOULD use `application/x-yaml` consistently (not `application/yaml` or `text/yaml`) for interoperability.

> **Digest semantics (NORMATIVE):**
>
> - **ETag** = `X-Pack-Digest` value. Enables conditional GET; same digest in lockfile, pinned refs, and headers.
> - **Content-Digest** (RFC 9530): digest of the message body **after content-coding is decoded** (bytes presented to the application). Used to detect transport tampering; MAY differ from canonical if server reformats.
> - **X-Pack-Digest**: digest of the canonical form (strict YAML → JSON → JCS). CLI MUST verify this after fetch.

**Error Responses:**

| Code | Meaning | Body |
|------|---------|------|
| 401 | Unauthorized | `{"error": "authentication_required"}` |
| 403 | Forbidden | `{"error": "license_expired"}` or `{"error": "pack_not_licensed"}` |
| 404 | Not Found | `{"error": "pack_not_found"}` |
| 410 | Gone | `{"error": "security_revocation", "reason": "...", "safe_version": "1.2.1"}` |
| 413 | Payload Too Large | `{"error": "pack_exceeds_size_limit"}` |
| 429 | Too Many Requests | `{"error": "rate_limit_exceeded", "retry_after": 60}` |

**410 Gone semantics (NORMATIVE):**

`410` is reserved for **security revocation** (pack pulled due to vulnerability/incident),
NOT for deprecation. Deprecated versions return `200` with `deprecated: true` in metadata.

**CLI behavior on 410:**

```
Error: Pack 'eu-ai-act-pro@1.1.0' has been revoked due to security issue.
Reason: CVE-2026-1234 - rule bypass vulnerability
Safe version: 1.2.1

To proceed anyway (forensics only): --allow-revoked
```

The `--allow-revoked` flag MUST:
- Require explicit opt-in (flag set by user)
- Log a warning to stderr
- When the client detects a CI environment (e.g. `CI=true` or `GITHUB_ACTIONS=true`), the client MUST also require `ASSAY_ALLOW_REVOKED=forensics` to be set; otherwise treat use of `--allow-revoked` in CI as error. Outside CI, the flag alone is sufficient (with stderr warning).

#### HEAD /packs/{name}/{version}

Metadata-only request (no body). Use for cache validation or digest lookup.

**Request:**

```http
HEAD /packs/eu-ai-act-pro/1.2.0 HTTP/1.1
Host: registry.getassay.dev
Authorization: Bearer <token>
```

**Response (200 OK):**

```http
HTTP/1.1 200 OK
ETag: "sha256:abc123..."
X-Pack-Digest: sha256:abc123...
X-Pack-Policy: commercial
X-Pack-Key-Id: sha256:def456...
X-Pack-License: LicenseRef-Assay-Enterprise-1.0
Content-Length: 4096
```

**Use cases:**
- Pre-flight check before download
- Digest lookup for lockfile generation
- Cache validation without full fetch

#### GET /packs/{name}/versions

List available versions.

**Response (200 OK):**

```json
{
  "name": "eu-ai-act-pro",
  "versions": [
    {"version": "1.2.0", "released": "2026-01-15", "deprecated": false, "digest": "sha256:abc..."},
    {"version": "1.1.0", "released": "2025-11-01", "deprecated": true, "digest": "sha256:def..."}
  ],
  "latest": "1.2.0"
}
```

#### GET /packs

List available packs (with pagination).

**Request:**

```http
GET /packs?limit=50&cursor=abc123 HTTP/1.1
```

**Response (200 OK):**

```json
{
  "packs": [
    {"name": "eu-ai-act-pro", "latest": "1.2.0", "license": "commercial"},
    {"name": "soc2-pro", "latest": "1.0.0", "license": "commercial"}
  ],
  "next_cursor": "def456",
  "has_more": true
}
```

**Pagination parameters:**

| Parameter | Default | Max | Description |
|-----------|---------|-----|-------------|
| `limit` | 50 | 100 | Results per page |
| `cursor` | - | - | Opaque cursor for next page |

### 4.4 Rate Limiting

| Endpoint | Limit | Window |
|----------|-------|--------|
| GET /packs/{name}/{version} | 100 | 1 minute |
| GET /packs | 20 | 1 minute |
| Total per token | 500 | 1 minute |

**Response headers:**

```http
X-RateLimit-Limit: 100
X-RateLimit-Remaining: 95
X-RateLimit-Reset: 1706529600
Retry-After: 30
```

---

## 5. Authentication

### 5.1 Token Authentication

```bash
# Environment variable
export ASSAY_REGISTRY_TOKEN=ast_...

# Or config file
assay config set registry.token ast_...
```

**Token format**: Opaque string, prefixed `ast_` (Assay Token).

**Token properties:**

| Property | Value |
|----------|-------|
| Prefix | `ast_` |
| Recommended TTL | ≤ 24h for CI, ≤ 90 days for dev |
| Revocable | Yes, via registry admin |
| Scoped | Optional (e.g., `packs:read`, `org:acme`) |

### 5.2 OIDC Authentication (GitHub Actions)

For CI/CD environments with OIDC, use **token exchange** (RECOMMENDED) rather than
passing the OIDC ID token directly as bearer.

#### 5.2.1 Token Exchange Flow (NORMATIVE)

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   GitHub    │     │    CLI      │     │  Registry   │
│   Actions   │     │             │     │             │
└──────┬──────┘     └──────┬──────┘     └──────┬──────┘
       │                   │                   │
       │ 1. Request OIDC   │                   │
       │    ID token       │                   │
       │<──────────────────│                   │
       │                   │                   │
       │ 2. ID token       │                   │
       │   (aud=registry)  │                   │
       │──────────────────>│                   │
       │                   │                   │
       │                   │ 3. POST /auth/oidc/exchange
       │                   │    { id_token: "..." }
       │                   │──────────────────>│
       │                   │                   │
       │                   │ 4. { access_token: "ast_...",
       │                   │      expires_in: 3600 }
       │                   │<──────────────────│
       │                   │                   │
       │                   │ 5. GET /packs/...
       │                   │    Authorization: Bearer ast_...
       │                   │──────────────────>│
```

**Why token exchange:**

- Registry can enforce scopes (e.g., `packs:read`, `org:acme`)
- Registry can revoke/rotate without GitHub-side changes
- Shorter token lifetime (10-60 min vs GitHub's ~15 min ID token)
- Prevents accidental ID token leakage in logs

#### 5.2.2 Exchange Endpoint

**POST /auth/oidc/exchange**

```http
POST /auth/oidc/exchange HTTP/1.1
Host: registry.getassay.dev
Content-Type: application/json

{
  "id_token": "<GitHub OIDC ID token>",
  "scope": "packs:read"
}
```

**Response (200 OK):**

```json
{
  "access_token": "ast_abc123...",
  "token_type": "Bearer",
  "expires_in": 3600,
  "scope": "packs:read"
}
```

**Error Responses:**

| Code | Error | Description |
|------|-------|-------------|
| 400 | `invalid_request` | Missing or malformed id_token |
| 401 | `invalid_token` | ID token expired, invalid signature, wrong audience |
| 403 | `access_denied` | Subject not authorized for requested scope |

#### 5.2.3 GitHub Actions Example

```yaml
permissions:
  id-token: write

steps:
  - name: Authenticate to Assay Registry
    run: |
      # 1. Get GitHub OIDC ID token
      ID_TOKEN=$(curl -s -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
        "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=https://registry.getassay.dev" | jq -r '.value')

      # 2. Exchange for registry access token
      RESPONSE=$(curl -s -X POST https://registry.getassay.dev/v1/auth/oidc/exchange \
        -H "Content-Type: application/json" \
        -d "{\"id_token\": \"$ID_TOKEN\", \"scope\": \"packs:read\"}")

      # 3. Extract access token
      ACCESS_TOKEN=$(echo "$RESPONSE" | jq -r '.access_token')
      echo "ASSAY_REGISTRY_TOKEN=$ACCESS_TOKEN" >> $GITHUB_ENV
```

#### 5.2.4 Registry OIDC Configuration

| Field | Value |
|-------|-------|
| Issuer | `https://token.actions.githubusercontent.com` |
| Audience | `https://registry.getassay.dev` |
| JWKS URI | `https://token.actions.githubusercontent.com/.well-known/jwks` |

**Subject claim patterns (NORMATIVE):**

Registry MUST support flexible subject matching, not hardcoded `refs/heads/main`:

| Pattern | Matches |
|---------|---------|
| `repo:ORG/REPO:*` | Any ref in repo |
| `repo:ORG/REPO:ref:refs/heads/*` | Any branch |
| `repo:ORG/REPO:ref:refs/heads/main` | Specific branch |
| `repo:ORG/REPO:environment:production` | Specific environment |

#### 5.2.5 Token Handling

- CLI MUST handle token expiry gracefully (re-exchange on 401)
- CLI SHOULD allow 30s clock skew tolerance
- CLI MUST implement exponential backoff on exchange failures (1s, 2s, 4s, max 30s)
- CLI MUST cache exchanged token until `expires_in - 60s`

### 5.3 No Authentication (Open Packs)

Open packs MAY be served without authentication for convenience:

```http
GET /packs/eu-ai-act-baseline/1.0.0 HTTP/1.1
# No Authorization header required
```

---

## 6. Integrity Verification

### 6.1 Pack Canonical Form (NORMATIVE)

YAML parsing is notoriously inconsistent. To ensure deterministic digests, packs MUST
conform to a strict subset:

**Strict YAML subset (NORMATIVE):**

| Feature | Status |
|---------|--------|
| Strings | ✅ Allowed |
| Integers | ✅ Allowed (JSON-representable: magnitude ≤ 2^53; negatives allowed) |
| Booleans, null | ✅ Allowed |
| Arrays, objects | ✅ Allowed |
| Floats | ⚠️ SHOULD avoid (see below) |
| Duplicate keys (at any nesting level) | ❌ MUST reject — parsing MUST fail. Implementations MUST detect duplicate keys (e.g. pre-scan or parser that reports duplicates); "last key wins" is NOT compliant. |
| Anchors/aliases | ❌ MUST reject |
| Tags (!!timestamp, !!binary, etc.) | ❌ MUST reject |
| Multi-document (---) | ❌ MUST reject |

**Number semantics (NORMATIVE):**

- Integers MUST be representable as JSON numbers (magnitude ≤ 2^53; use string for larger values).
- Floats SHOULD be avoided; use strings for precise decimals (e.g. `"0.95"`).
- If floats are used, they MUST be finite and MUST survive `parse → JCS → parse` round-trip losslessly (per RFC 8785 number rules).
- Leading zeros, exponent notation: normalize per JCS (RFC 8785 §3.2.4).

**Canonical bytes (NORMATIVE):** The output of the canonicalization step (JCS) used for digest and DSSE payload MUST be UTF-8 encoded, with no BOM and no trailing newline. This ensures interoperability across implementations (e.g. JCS libraries that append newlines are non-compliant unless they strip before use).

**Canonicalization algorithm:**

```
1. Parse YAML in strict mode: reject duplicate keys, anchors/aliases, unknown tags, multi-document
2. Convert to JSON value (strings, numbers, bools, null, arrays, objects)
3. Apply JCS canonicalization (RFC 8785); output = UTF-8, no BOM, no trailing newline
4. Compute SHA-256 hash
5. Format as "sha256:{hex_digest}"
```

**Implementation:** Use a parser or pre-pass that guarantees duplicate-key detection and rejection. Parser configurations that silently take "last key wins" are non-compliant. Conformance test: a pack with duplicate keys at any level MUST be rejected before digest computation.

### 6.2 Digest Verification (NORMATIVE)

After fetching, CLI MUST verify digest:

```rust
let fetched_content = fetch_pack(url)?;
let canonical = jcs_canonicalize(parse_yaml_strict(fetched_content)?)?;
let computed_digest = format!("sha256:{}", sha256_hex(&canonical));
let expected_digest = response.header("X-Pack-Digest");

if computed_digest != expected_digest {
    return Err(PackIntegrityError::DigestMismatch {
        expected: expected_digest,
        computed: computed_digest,
    });
}
```

### 6.3 Signature Verification (NORMATIVE)

Digest alone provides **integrity** but not **authenticity**. A compromised registry
could serve malicious content with matching digest.

**Verification requirements:** The client determines "signature required" from the **`X-Pack-Policy`** response header (§4.3), not from pack body content. When `X-Pack-Policy: commercial`, the client MUST verify signature; when `open`, signature is optional.

| Pack Type | Signature Verification |
|-----------|----------------------|
| Commercial (registry; `X-Pack-Policy: commercial`) | MUST verify |
| Open (registry; `X-Pack-Policy: open`) | SHOULD verify |
| BYOS | SHOULD verify (if signature present; see §9) |
| Local file | MAY verify |

#### 6.3.1 DSSE Envelope Format (NORMATIVE)

`X-Pack-Signature` contains a **Base64-encoded DSSE envelope** (not raw signature):

```json
{
  "payloadType": "application/vnd.assay.pack.v1+jcs",
  "payload": "<base64-encoded-canonical-bytes>",
  "signatures": [
    {
      "keyid": "sha256:def456...",
      "sig": "<base64-encoded-Ed25519-signature>"
    }
  ]
}
```

**Fields:**

| Field | Description |
|-------|-------------|
| `payloadType` | MUST be `application/vnd.assay.pack.v1+jcs` |
| `payload` | Base64-encoded canonical bytes (from §6.1: UTF-8, no BOM, no trailing newline) |
| `signatures[].keyid` | SHA256 of SPKI public key (matches `X-Pack-Key-Id`) |
| `signatures[].sig` | Ed25519 signature over PAE(payloadType, payload) |

**PAE (Pre-Authentication Encoding):**

```
PAE(payloadType, payload) =
  "DSSEv1" + SP +
  len(payloadType) + SP + payloadType + SP +
  len(payload) + SP + payload
```

Where `SP` = space (0x20), `len()` = decimal string of byte length.

#### 6.3.2 Verification Flow

```rust
// 1. Decode DSSE envelope from header
let envelope: DsseEnvelope = base64_decode_json(response.header("X-Pack-Signature")?)?;

// 2. Verify payloadType
assert_eq!(envelope.payload_type, "application/vnd.assay.pack.v1+jcs");

// 3. Verify payload matches canonical content
let canonical = jcs_canonicalize(parse_yaml_strict(content)?)?;
assert_eq!(base64_decode(&envelope.payload)?, canonical);

// 4. Get trusted public key
let key_id = &envelope.signatures[0].keyid;
let public_key = trust_store.get_key(key_id)?;

// 5. Verify signature over PAE
let pae = dsse_pae(&envelope.payload_type, &envelope.payload);
verify_ed25519(public_key, &pae, &envelope.signatures[0].sig)?;
```

#### 6.3.3 Signature Sidecar Endpoint (RECOMMENDED)

**Problem**: DSSE envelopes include the payload (canonical bytes), which can exceed HTTP
header limits (~8KB) for larger packs. Reverse proxies and CDNs may silently truncate
or reject oversized headers.

**Solution**: Deliver signatures via a sidecar endpoint instead of headers.

**Endpoint (NORMATIVE):**

```http
GET /packs/{name}/{version}.sig
Authorization: Bearer <token>

HTTP/1.1 200 OK
Content-Type: application/vnd.dsse.envelope+json

{
  "payloadType": "application/vnd.assay.pack.v1+jcs",
  "payload": "<base64-encoded-canonical-bytes>",
  "signatures": [
    {
      "keyid": "sha256:def456...",
      "sig": "<base64-encoded-Ed25519-signature>"
    }
  ]
}
```

**Response codes:**

| Status | Meaning |
|--------|---------|
| 200 | Signature available (DSSE envelope in body) |
| 404 | Signature not available. For **commercial** packs: client MUST treat as failure (pack unsigned or registry misconfiguration). For **open** packs: client MAY proceed without signature. |
| 401 | Authentication required |

**Commercial packs (NORMATIVE):** Registry MUST provide a signature (either `X-Pack-Signature` header or sidecar GET `/packs/{name}/{version}.sig` returning 200). If the client resolves the pack as commercial and neither header nor sidecar yields a valid signature (e.g. sidecar returns 404), the client MUST reject the pack and MUST NOT use it. "Unsigned" is not allowed for commercial.

**Client behavior (NORMATIVE):**

1. When `X-Pack-Signature-Endpoint` is present (e.g. `/packs/{name}/{version}.sig`), client SHOULD prefer fetching the signature from the sidecar (avoids header size limits); client MAY try `X-Pack-Signature` header first for backward compatibility.
2. If header absent or invalid, client MUST fetch from sidecar endpoint when the pack is commercial or when signature is required.
3. For commercial packs: missing signature (header absent and sidecar 404 or invalid) MUST result in hard failure.
4. `fetch_pack_with_signature()` may fetch content and signature in parallel.

**Registry behavior:**

- Registries MUST support the sidecar endpoint `GET /packs/{name}/{version}.sig` for all signed packs.
- Registries MAY also include `X-Pack-Signature` header for small packs (<4KB).
- Registries SHOULD set `X-Pack-Signature-Endpoint: /packs/{name}/{version}.sig` to indicate sidecar availability (path MUST match the GET endpoint above).

**Header size guidance:**

| Pack Size | Canonical Size | Envelope Size | Delivery |
|-----------|---------------|---------------|----------|
| < 4KB     | < 3KB         | < 4KB         | Header OK |
| 4KB-100KB | 3-75KB        | 4-100KB       | Sidecar REQUIRED |
| > 100KB   | > 75KB        | > 100KB       | Sidecar REQUIRED |

**Security note**: The sidecar endpoint requires the same authentication as the pack
endpoint. Signature must be verified against the content digest, not just presence.

### 6.4 Key Trust Model (NORMATIVE)

TLS + registry allowlist is necessary but insufficient for enterprise trust.

#### 6.4.1 Trust Roots (NORMATIVE)

**Hierarchy:**

1. **Embedded roots (baseline)** — The CLI binary MAY ship with a set of pinned root public key IDs (e.g. Assay production signing keys). These are used to verify the registry keys manifest (§6.4.2). No TOFU: keys manifest MUST be signed by an embedded root (or by a key from config, see below).
2. **Config roots (add/override)** — User or deployment can add roots via config (e.g. `~/.assay/config.toml` or `ASSAY_REGISTRY_TRUST_ROOTS`). **Default combine strategy (NORMATIVE):** embedded ∪ config (union). All listed roots are trusted. An optional **override** mode (config-only, ignoring embedded) is permitted only when explicitly set (e.g. `registry.trust.mode = "override"`); implementations MUST document this and SHOULD require explicit opt-in to avoid accidentally disabling embedded roots (e.g. in CI images).
3. **Keys manifest** — Pack-signing keys in the `/keys` manifest are verified by (embedded or config) roots. Only keys that chain to a root are trusted for pack signature verification.

**Example config (informative):**

```toml
# ~/.assay/config.toml — optional; embedded roots suffice for default registry
[registry.trust]
roots = [
  "sha256:abc123...",  # Assay signing key 2026
  "sha256:def456...",  # Assay signing key 2025 (rotation)
]
# Optional: mode = "override" to use only config roots (use with care)
```

#### 6.4.2 Keys Manifest

Registry publishes a signed keys manifest at `/keys`:

```http
GET /keys HTTP/1.1
Host: registry.getassay.dev
```

**Response:**

```json
{
  "keys": [
    {
      "id": "sha256:abc123...",
      "algorithm": "Ed25519",
      "public_key": "<base64-SPKI>",
      "not_before": "2026-01-01T00:00:00Z",
      "not_after": "2027-01-01T00:00:00Z",
      "usage": ["pack-signing"]
    }
  ],
  "signature": "<DSSE envelope over this manifest>"
}
```

**Manifest signature (NORMATIVE):** The keys manifest DSSE envelope MUST use `payloadType` exactly `application/vnd.assay.registry.keys.v1+json`. The DSSE payload is the UTF-8 bytes of the JSON response body as served (no additional JCS canonicalization); authenticity is provided by the DSSE signature. The manifest MUST be signed by a key whose key id is listed in the client's trust roots (embedded or config). Implementations MUST verify the manifest signature against one of those roots before trusting any key in `keys[]`.

**Verification:**

1. CLI fetches `/keys` manifest
2. Verifies manifest signature against pinned root
3. Caches manifest (TTL from `Cache-Control`, default 24h)
4. Uses manifest keys to verify pack signatures

This provides:
- Key rotation without CLI updates
- No TOFU (Trust On First Use)
- Explicit validity periods

#### 6.4.3 Trust Hierarchy

| Source | Trust Level |
|--------|-------------|
| Pinned root (CLI binary) | Highest - verifies keys manifest |
| Keys manifest entry | High - verified by root |
| Config-provided key | High - explicit user trust |
| Unknown key | MUST reject for commercial |

### 6.5 Keyless Signing (Future)

For OIDC-based keyless signing (Sigstore/Fulcio model):

```http
X-Pack-Signature: <DSSE envelope with certificate>
X-Pack-Certificate: <base64-Fulcio-cert>
X-Pack-Transparency-Log: <rekor-entry-url>
```

This is planned for v1.1 and aligns with the attestation positioning in ADR-018.

---

## 7. Caching

### 7.1 Local Cache

Cache layout MUST avoid collisions when multiple registries or namespaces are used. Recommended structure:

```
~/.assay/cache/packs/{registry_id}/{namespace}/{name}/{version}/pack.yaml
~/.assay/cache/packs/{registry_id}/{namespace}/{name}/{version}/metadata.json
~/.assay/cache/packs/{registry_id}/{namespace}/{name}/{version}/signature.json
```

Where `registry_id` is a stable identifier for the registry (e.g. hostname or hash of base URL) and `namespace` is the org path if present (e.g. `orgs/acme` or `_global`). For a single default registry and no namespace, implementations MAY use the shorter path `~/.assay/cache/packs/{name}/{version}/` for backward compatibility, but MUST document that adding a second registry or namespace requires the extended path to avoid privilege mixing.

**metadata.json:**

```json
{
  "fetched_at": "2026-01-29T10:00:00Z",
  "digest": "sha256:abc123...",
  "etag": "\"sha256:abc123...\"",
  "expires_at": "2026-01-30T10:00:00Z",
  "registry_url": "https://registry.getassay.dev/v1",
  "policy": "commercial",
  "key_id": "sha256:def456..."
}
```

`policy` MUST be the value of `X-Pack-Policy` from the response (used on cache read to decide signature requirement).

**signature.json:**

Cached DSSE envelope for offline verification.

### 7.2 Cache Integrity Verification (NORMATIVE)

**On every cache read**, CLI MUST verify integrity before use:

```rust
fn load_cached_pack(name: &str, version: &str) -> Result<Pack> {
    let cache_dir = cache_path(name, version);
    let content = fs::read(cache_dir.join("pack.yaml"))?;
    let metadata: Metadata = load_json(cache_dir.join("metadata.json"))?;

    // 1. Verify digest (guards against disk corruption/tampering)
    let canonical = jcs_canonicalize(parse_yaml_strict(&content)?)?;
    let computed = format!("sha256:{}", sha256_hex(&canonical));
    if computed != metadata.digest {
        // Cache corrupted - evict and re-fetch
        evict_cache(name, version);
        return Err(CacheCorrupted { name, version });
    }

    // 2. Signature: for commercial packs, signature MUST be present and valid (see §6.3)
    if pack_requires_signature(&metadata) {
        let envelope = load_json::<DsseEnvelope>(cache_dir.join("signature.json"))
            .map_err(|_| PackError::MissingSignature { name, version })?;
        verify_dsse(&envelope, &canonical, &trust_store)?;
    } else if let Ok(envelope) = load_json::<DsseEnvelope>(cache_dir.join("signature.json")) {
        verify_dsse(&envelope, &canonical, &trust_store)?;
    }

    parse_pack(&content)
}
```

**Commercial packs (NORMATIVE):** The client MUST record `X-Pack-Policy` (or equivalent) in cache metadata when storing a pack. On cache read, when metadata indicates `policy: commercial` (or signature required per §6.3), missing or invalid `signature.json` MUST result in failure: evict cache and re-fetch, or return error. The client MUST NOT use the pack without a valid signature.

**Rationale**: Local disk is not trusted. Malware, disk errors, or user mistakes
could modify cached packs.

### 7.3 Cache Invalidation

| Scenario | Behavior |
|----------|----------|
| Cache hit, not expired, integrity OK | Use cached |
| Cache hit, not expired, integrity FAIL | Evict, re-fetch |
| Cache hit, expired | Re-fetch with `If-None-Match`, verify |
| Cache miss | Fetch, verify, cache |
| `--no-cache` flag | Always fetch, verify |

### 7.4 Cache TTL

Default: 24 hours (86400 seconds), overridable via `Cache-Control` header.

**Cache-Control requirements:**

| Pack Type | Cache-Control | Vary |
|-----------|---------------|------|
| Commercial (authenticated) | `private, max-age=86400` | `Authorization, Accept-Encoding` |
| Open (unauthenticated) | `public, max-age=86400` | `Accept-Encoding` |

> **Security note**: Commercial packs MUST use `Cache-Control: private` and
> `Vary: Authorization` to prevent caching by intermediate proxies that don't
> understand authorization context.

---

## 8. Lockfile

### 8.1 Purpose

Enterprise pipelines need reproducible builds. The lockfile captures resolved pack
references with full verification metadata.

### 8.2 Lockfile Format

**Filename**: `assay.packs.lock` (or `assay.lock` with `[packs]` section)

```yaml
# assay.packs.lock
# DO NOT EDIT - Generated by assay pack lock
version: 2
generated_at: "2026-01-29T10:00:00Z"
generated_by: "assay-cli/2.12.0"

packs:
  - name: eu-ai-act-pro
    version: "1.2.0"
    digest: sha256:abc123...
    source: registry
    registry_url: "https://registry.getassay.dev/v1"
    namespace: null  # or "orgs/acme"
    fetched_at: "2026-01-29T10:00:00Z"
    etag: "\"sha256:abc123...\""
    signature:
      algorithm: Ed25519
      key_id: sha256:def456...

  - name: eu-ai-act-baseline
    version: "1.0.0"
    digest: sha256:789xyz...
    source: bundled

  - name: custom-rules
    version: "1.0.0"
    digest: sha256:qrs789...
    source: byos
    byos_url: "s3://my-bucket/packs/custom.yaml"
```

**Version 2 fields (new):**

| Field | Description |
|-------|-------------|
| `registry_url` | Exact registry used (for multi-registry setups) |
| `namespace` | Organization namespace if used |
| `etag` | HTTP ETag for conditional requests |
| `signature.algorithm` | Signature algorithm (future-proofing) |
| `byos_url` | BYOS source URL (for BYOS packs) |

### 8.3 CLI Commands

```bash
# Generate/update lockfile from current packs
assay pack lock

# Verify current packs match lockfile (digest + signature)
assay pack lock --verify

# CI mode: fail if lockfile outdated or verification fails
assay pack lock --check

# Update lockfile (re-fetch all, update digests)
assay pack lock --update
```

### 8.4 Lockfile Behavior

| Flag | Behavior |
|------|----------|
| (none) | Create lockfile if missing, error if exists and outdated |
| `--verify` | Verify all packs match lockfile, exit 0/1 |
| `--check` | Verify + fail if lockfile needs update (CI mode) |
| `--update` | Re-fetch all packs, update lockfile |

### 8.5 CI Integration

```yaml
steps:
  - name: Verify pack lockfile
    run: assay pack lock --check

  - name: Lint with locked packs
    run: assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz
```

**Lockfile enforcement (NORMATIVE):**

When `assay.packs.lock` exists:

| Scenario | Behavior |
|----------|----------|
| Digest matches | Use pack |
| Digest differs | Error with diff, suggest `--update` |
| Pack missing from lockfile | Error, suggest `assay pack lock` |
| Pack in lockfile but not requested | Warning only |

### 8.6 Security Revocation Handling

If a locked pack version is revoked (410):

```
Error: Pack 'eu-ai-act-pro@1.1.0' in lockfile has been revoked.

Reason: CVE-2026-1234 - rule bypass vulnerability
Safe version: 1.2.1

To update lockfile: assay pack lock --update
To proceed anyway: assay pack lock --allow-revoked  # forensics only
```

---

## 9. BYOS Pack Storage

Users can host packs in their own storage:

```bash
# S3
assay evidence lint --pack s3://my-bucket/packs/custom.yaml bundle.tar.gz

# GCS
assay evidence lint --pack gs://my-bucket/packs/custom.yaml bundle.tar.gz

# Azure
assay evidence lint --pack az://container/packs/custom.yaml bundle.tar.gz
```

**Authentication**: Uses same OIDC/credentials as BYOS evidence push.

**Integrity**: BYOS packs SHOULD provide a signature when possible. Signature and digest semantics are implementation-defined for BYOS (e.g. object metadata, sidecar file, or a well-known YAML field such as `x-assay-sig` in the pack root for inline signature reference). The **canonical spelling** for a pack-root YAML field, if used, is `x-assay-sig` (lowercase, hyphen). The client MUST have an expected digest to verify against:
either a **pinned ref** (e.g. `s3://bucket/packs/custom.yaml#sha256:...`) or an entry in
`assay.packs.lock` with `source: byos` and `digest`. Use of BYOS without a pin or lockfile
SHOULD trigger a warning; implementations MAY reject or allow with downgraded assurance.

---

## 10. Error Messages

### 10.1 User-Facing Errors

| Error | Message |
|-------|---------|
| Not found | `Pack 'eu-ai-act-pro@1.2.0' not found. Check pack name and version.` |
| Auth required | `Pack 'eu-ai-act-pro' requires authentication. Set ASSAY_REGISTRY_TOKEN or configure OIDC.` |
| Not licensed | `Pack 'eu-ai-act-pro' is not included in your license. Contact sales@getassay.dev` |
| Digest mismatch | `Pack integrity check failed. Expected sha256:abc..., got sha256:def...` |
| Deprecated | `Pack 'eu-ai-act-pro@1.1.0' is deprecated. Use @1.2.0 instead.` |

### 10.2 GitHub Action Error

When registry fetch fails in Action context:

```yaml
- name: Lint with pack
  run: |
    if ! assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz; then
      echo "::error::Pack fetch failed. See https://getassay.dev/docs/enterprise-packs"
      exit 1
    fi
```

---

## 11. CLI Implementation

### 11.1 Config Commands

```bash
# Set registry token
assay config set registry.token ast_...

# Set custom registry URL
assay config set registry.url https://registry.example.com/v1

# Clear cache
assay cache clear packs

# List cached packs
assay cache list packs
```

### 11.2 Fetch Command (Optional)

```bash
# Pre-fetch pack for offline use
assay pack fetch eu-ai-act-pro@1.2.0

# Verify pack integrity
assay pack verify eu-ai-act-pro@1.2.0
```

---

## 12. Security Considerations

### 12.1 Token Security

- Tokens SHOULD be short-lived (< 24h for CI)
- Tokens MUST NOT be logged
- OIDC preferred over long-lived tokens

### 12.2 MITM Protection

- Registry MUST use HTTPS
- CLI MUST verify TLS certificates
- Digest verification provides content integrity
- Signature verification provides authenticity

### 12.3 Supply Chain

- Pack digests are computed from JCS-canonical content
- For **signed commercial packs**, unknown YAML fields MUST cause validation failure (no injection via ignored fields). For open packs or future schema evolution, implementations MAY treat unknown fields as lint warnings rather than hard failure if pack schema versioning allows additive extensions; normative behavior for commercial remains strict reject.
- **Commercial pack schema evolution:** Adding new top-level or rule-level fields to the pack schema is **breaking** for commercial packs unless a pack schema version is introduced and clients opt-in. Commercial packs are versioned; additive schema changes require a bump of pack schema version and client support; otherwise existing commercial packs would break validation. This SPEC does not define pack schema versioning; it is stated here to avoid the assumption that "add a field, minor bump" is safe for commercial.
- Signed packs provide author verification
- Commercial packs MUST be signed (see §6.3)

### 12.4 YAML Parsing Security

YAML parsers are vulnerable to DoS attacks. The following limits are **implementation guidance**; implementations SHOULD enforce them to prevent resource exhaustion. For normative canonicalization rules, see §6.1.

| Attack | Mitigation |
|--------|------------|
| Billion laughs (anchor expansion) | Reject anchors/aliases (normative in §6.1) |
| Deep nesting | Limit depth to 50 |
| Huge strings | Limit string length to 1MB |
| Many keys | Limit object keys to 10,000 |

**Implementation**: Use a parser with recursion/depth limits or validate structure before parsing; duplicate keys MUST be rejected (normative in §6.1).

### 12.5 Size Limits

The following are **implementation guidance** (SHOULD) unless a future version makes them normative:

| Limit | Value | Rationale |
|-------|-------|-----------|
| Max pack size | 10 MB | Prevent DoS, reasonable for rule sets |
| Max rules per pack | 1,000 | Performance |
| Max string field | 1 MB | Prevent memory exhaustion |

CLI SHOULD support `Accept-Encoding: gzip` and decompress transparently.

---

## 13. Future: OCI Distribution

### 13.1 Rationale

OCI (Open Container Initiative) registries are increasingly used for non-container artifacts
via ORAS (OCI Registry As Storage). Benefits:

- Existing auth infrastructure (Docker Hub, GHCR, ECR, etc.)
- Built-in signing via cosign/Sigstore
- Mirrors, caching, air-gapped support
- Ecosystem tooling

### 13.2 Planned Support (v1.1)

```bash
# OCI pull (by tag - discovery only)
assay pack pull oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0

# OCI pull (by digest - CI/builds)
assay pack pull oci://ghcr.io/assay/packs/eu-ai-act-pro@sha256:abc123...

# Verify cosign signature
assay pack verify oci://ghcr.io/assay/packs/eu-ai-act-pro@sha256:abc123...
```

**OCI artifact layout (NORMATIVE for v1.1):**

| Layer | Media Type | Content |
|-------|------------|---------|
| Config | `application/vnd.assay.pack.config.v1+json` | Pack metadata (name, version, license) |
| Layer 0 | `application/vnd.assay.pack.content.v1+yaml` | Pack YAML content |

**Manifest annotations:**

| Annotation | Value |
|------------|-------|
| `dev.assay.pack.name` | Pack name |
| `dev.assay.pack.version` | Semver version |
| `dev.assay.pack.digest` | Canonical digest (X-Pack-Digest equivalent) |

### 13.3 Digest Mapping

| HTTP Registry | OCI Registry |
|---------------|--------------|
| `X-Pack-Digest` | Manifest annotation + layer digest |
| `X-Pack-Signature` | cosign signature (attached or DSSE) |
| `X-Pack-Key-Id` | cosign public key / keyless identity |

**Signing (NORMATIVE for v1.1):**

```bash
# Sign with cosign (keyless)
cosign sign --yes ghcr.io/assay/packs/eu-ai-act-pro@sha256:abc123...

# Verify
cosign verify ghcr.io/assay/packs/eu-ai-act-pro@sha256:abc123... \
  --certificate-identity=release@assay.dev \
  --certificate-oidc-issuer=https://accounts.google.com
```

### 13.4 CI Best Practice

**Tags are mutable; digests are immutable.** For reproducible builds:

```yaml
# Discovery (get latest version)
- run: |
    DIGEST=$(assay pack resolve oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0)
    echo "PACK_DIGEST=$DIGEST" >> $GITHUB_ENV

# Build (use digest)
- run: assay evidence lint --pack "oci://ghcr.io/assay/packs/eu-ai-act-pro@$PACK_DIGEST" bundle.tar.gz
```

### 13.5 Coexistence

HTTP registry and OCI distribution will coexist. **OCI is an additional registry resolver class;** it does not change the resolution order of path → built-in → local config dir → registry → BYOS (per SPEC-Pack-Engine-v1). When OCI is supported, "registry" is interpreted to include both HTTP registry and OCI registry (order between them is implementation-defined, typically HTTP before OCI).

```bash
# HTTP registry (default)
--pack eu-ai-act-pro@1.2.0

# OCI registry (explicit)
--pack oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0
```

Resolution order with OCI (informative):

```
1. Local path      ./custom.yaml
2. Bundled pack    eu-ai-act-baseline
3. HTTP registry   eu-ai-act-pro@1.2.0
4. OCI registry    oci://ghcr.io/.../pack:1.2.0
5. BYOS            s3://bucket/packs/...
```

---

## 14. Implementation Checklist

### Phase 1 (v1.0)

- [ ] Pack resolution order in CLI (per SPEC-Pack-Engine-v1)
- [ ] Registry client with token auth
- [ ] OIDC token exchange endpoint (`POST /auth/oidc/exchange`)
- [ ] Strict YAML parsing (reject anchors, duplicates, tags, floats; see §6.1)
- [ ] Digest verification (JCS canonical)
- [ ] DSSE envelope signature verification (MUST for commercial; missing signature = fail)
- [ ] Pinned root keys (embedded + config hierarchy) + keys manifest fetch
- [ ] Cache integrity verification on every read (commercial: signature required)
- [ ] Local caching with TTL + ETag/304 + Vary (cache path includes registry/namespace when applicable)
- [ ] Lockfile v2 support (`assay.packs.lock`) — format defined in §8
- [ ] HEAD endpoint support
- [ ] `assay config set registry.*` commands
- [ ] BYOS pack fetch (S3/GCS/Azure); expected digest from pin suffix or lockfile
- [ ] Error messages per §10
- [ ] Rate limit handling (429 + Retry-After)
- [ ] 410 revocation handling + `--allow-revoked` (CI detection per §4.3)

### Phase 2 (v1.1)

- [ ] Keyless signing (Sigstore/Fulcio)
- [ ] OCI distribution support (ORAS)
- [ ] cosign verification integration
- [ ] Organization namespaces

---

## 15. References

### Normative

- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)
- [RFC 8785 - JCS](https://datatracker.ietf.org/doc/html/rfc8785) — JSON Canonicalization Scheme
- [RFC 9530 - Digest Fields](https://datatracker.ietf.org/doc/html/rfc9530) — HTTP Content-Digest
- [DSSE](https://github.com/secure-systems-lab/dsse) — Dead Simple Signing Envelope

### Informative

- [Sigstore](https://sigstore.dev) — Keyless signing
- [ORAS](https://oras.land) — OCI Registry As Storage
- [OPA Bundle Distribution](https://www.openpolicyagent.org/docs/latest/management-bundles/) — Similar pattern
- [TUF](https://theupdateframework.io) — Update security framework
- [cosign](https://docs.sigstore.dev/cosign/overview/) — Container signing
