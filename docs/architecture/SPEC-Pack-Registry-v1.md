# SPEC-Pack-Registry-v1

**Version:** 1.0.2
**Status:** Draft
**Date:** 2026-01-29
**Related:** [ADR-016](./ADR-016-Pack-Taxonomy.md), [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)

## Abstract

This specification defines the pack registry protocol for resolving and fetching compliance packs
from remote sources. It enables enterprise pack distribution without including commercial content
in the open source repository.

---

## 1. Scope

### 1.1 In Scope

- Pack resolution order (local → bundled → registry)
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
| 1.0.2 | OIDC token exchange endpoint, DSSE envelope format, Content-Digest (RFC 9530), Vary header, key trust manifest, number policy, HEAD endpoint, cache integrity verification, lockfile extensions, 410 handling |
| 1.0.1 | Add signature verification (MUST for commercial), strict canonicalization, lockfile, ETag/304, pagination, rate limits, OCI future track |
| 1.0.0 | Initial specification |

---

## 2. Pack Resolution Order

When `--pack <ref>` is specified, the CLI resolves in this order:

```
1. Local path     ./custom.yaml           → file exists? use it
2. Bundled pack   eu-ai-act-baseline      → in packs/open/? use it
3. Registry       eu-ai-act-pro@1.2.0     → fetch from registry
4. BYOS           s3://bucket/packs/...   → fetch from user storage
```

**Fail-fast**: If resolution fails at any step, error immediately with clear message.

---

## 3. Pack Reference Format

```
pack_ref := local_path | bundled_name | registry_ref | pinned_ref | byos_url

local_path    := "./" path ".yaml"
bundled_name  := identifier                    # e.g., "eu-ai-act-baseline"
registry_ref  := identifier "@" version        # e.g., "eu-ai-act-pro@1.2.0"
pinned_ref    := identifier "@" version "#" digest  # e.g., "eu-ai-act-pro@1.2.0#sha256:abc..."
byos_url      := scheme "://" path ".yaml"     # e.g., "s3://bucket/packs/custom.yaml"
```

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
| `ETag` | MUST | Strong ETag = X-Pack-Digest value (for conditional requests) |
| `Content-Digest` | MUST | RFC 9530 digest of wire bytes (may differ from canonical) |
| `X-Pack-Digest` | MUST | SHA256 digest of **canonical** content (JCS) |
| `X-Pack-Signature` | MUST (commercial) | Base64-encoded DSSE envelope (see §6.3) |
| `X-Pack-Key-Id` | MUST (if signed) | SHA256 of SPKI public key |
| `X-Pack-License` | MUST | SPDX identifier (use `LicenseRef-*` for custom) |
| `Cache-Control` | MUST | `private` for authenticated, `public` for open |
| `Vary` | MUST | `Authorization, Accept-Encoding` for authenticated responses |

> **Digest semantics (NORMATIVE):**
>
> - `Content-Digest` (RFC 9530): digest of the **wire representation** (what you received)
> - `X-Pack-Digest`: digest of the **canonical form** (after strict YAML parse + JCS)
>
> These MAY differ if the registry applies formatting. CLI MUST verify `X-Pack-Digest`.

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
- Require explicit opt-in (no env var override)
- Log a warning to stderr
- NOT be usable in CI without `ASSAY_ALLOW_REVOKED=forensics`

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

**Allowed YAML features:**

| Feature | Status |
|---------|--------|
| Strings | ✅ Allowed |
| Integers | ✅ Allowed (JSON-representable) |
| Booleans, null | ✅ Allowed |
| Arrays, objects | ✅ Allowed |
| Floats | ⚠️ SHOULD avoid (see below) |
| Duplicate keys | ❌ MUST error |
| Anchors/aliases | ❌ MUST error |
| Tags (!!timestamp, !!binary, etc.) | ❌ MUST error |
| Multi-document | ❌ MUST error |

**Number semantics (NORMATIVE):**

YAML → JSON number conversion is lossy for floats. To ensure determinism:

- Integers MUST be representable in JSON (≤ 2^53)
- Floats SHOULD be avoided; use strings for precise decimals (e.g., `"0.95"` not `0.95`)
- If floats are used, they MUST survive `parse → JCS → parse` round-trip losslessly
- Leading zeros, exponent notation: normalize per JCS (RFC 8785 §3.2.4)

**Canonicalization algorithm:**

```
1. Parse YAML (strict mode: reject features above)
2. Convert to JSON value (strings, numbers, bools, null, arrays, objects)
3. Apply JCS canonicalization (RFC 8785)
4. Compute SHA-256 hash
5. Format as "sha256:{hex_digest}"
```

**Implementation note**: Use a YAML parser with strict mode. If using `serde_yaml`,
disable anchors and reject unknown tags.

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

**Verification requirements:**

| Pack Type | Signature Verification |
|-----------|----------------------|
| Commercial (registry) | MUST verify |
| Open (registry) | SHOULD verify |
| BYOS | SHOULD verify (if `x-assay-sig` present) |
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
| `payload` | Base64-encoded canonical JSON bytes (from §6.1) |
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

### 6.4 Key Trust Model (NORMATIVE)

TLS + registry allowlist is necessary but insufficient for enterprise trust.

#### 6.4.1 Trust Roots

CLI ships with **pinned root public keys**:

```toml
# ~/.assay/config.toml
[registry.trust]
roots = [
  "sha256:abc123...",  # Assay signing key 2026
  "sha256:def456...",  # Assay signing key 2025 (rotation)
]
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

```
~/.assay/cache/packs/{name}/{version}/pack.yaml
~/.assay/cache/packs/{name}/{version}/metadata.json
~/.assay/cache/packs/{name}/{version}/signature.json
```

**metadata.json:**

```json
{
  "fetched_at": "2026-01-29T10:00:00Z",
  "digest": "sha256:abc123...",
  "etag": "\"sha256:abc123...\"",
  "expires_at": "2026-01-30T10:00:00Z",
  "registry_url": "https://registry.getassay.dev/v1",
  "key_id": "sha256:def456..."
}
```

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

    // 2. Verify signature if present (commercial packs)
    if let Ok(envelope) = load_json::<DsseEnvelope>(cache_dir.join("signature.json")) {
        verify_dsse(&envelope, &canonical, &trust_store)?;
    }

    parse_pack(&content)
}
```

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

**Integrity**: BYOS packs SHOULD include `x-assay-sig` for verification since there's
no registry-provided digest header.

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
- Unknown YAML fields cause validation failure (no injection via ignored fields)
- Signed packs provide author verification
- Commercial packs MUST be signed (see §6.3)

### 12.4 YAML Parsing Security

YAML parsers are vulnerable to DoS attacks via:

| Attack | Mitigation |
|--------|------------|
| Billion laughs (anchor expansion) | Reject anchors/aliases |
| Deep nesting | Limit depth to 50 |
| Huge strings | Limit string length to 1MB |
| Many keys | Limit object keys to 10,000 |

**Implementation**: Use `serde_yaml` with recursion limits or validate structure before parsing.

### 12.5 Size Limits

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

HTTP registry and OCI distribution will coexist:

```bash
# HTTP registry (default)
--pack eu-ai-act-pro@1.2.0

# OCI registry (explicit)
--pack oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0
```

Resolution order with OCI:

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

- [ ] Pack resolution order in CLI
- [ ] Registry client with token auth
- [ ] OIDC token exchange endpoint (`POST /auth/oidc/exchange`)
- [ ] Strict YAML parsing (reject anchors, duplicates, tags, floats)
- [ ] Digest verification (JCS canonical)
- [ ] DSSE envelope signature verification (MUST for commercial)
- [ ] Pinned root keys + keys manifest fetch
- [ ] Cache integrity verification on every read
- [ ] Local caching with TTL + ETag/304 + Vary
- [ ] HEAD endpoint support
- [ ] `assay config set registry.*` commands
- [ ] BYOS pack fetch (S3/GCS/Azure)
- [ ] Error messages per §10
- [ ] Rate limit handling (429 + Retry-After)
- [ ] 410 revocation handling + `--allow-revoked`

### Phase 2 (v1.1)

- [ ] Lockfile v2 support (`assay.packs.lock`)
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
