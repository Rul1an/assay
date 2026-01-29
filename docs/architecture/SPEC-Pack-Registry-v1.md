# SPEC-Pack-Registry-v1

**Version:** 1.0.1
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
- Authentication (OIDC, token)
- Integrity verification (digest + signature)
- Pack canonicalization
- Caching behavior
- Lockfile format

### 1.2 Out of Scope

- Pack content/rules (see SPEC-Pack-Engine-v1)
- Registry hosting implementation
- Billing/licensing enforcement

### 1.3 Changelog

| Version | Changes |
|---------|---------|
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
If-None-Match: "sha256:abc123..."
```

**Response (200 OK):**

```http
HTTP/1.1 200 OK
Content-Type: application/x-yaml
ETag: "sha256:abc123..."
Digest: sha-256=:q1b2c3...:
X-Pack-Digest: sha256:abc123...
X-Pack-Signature: <base64-encoded-signature>
X-Pack-Key-Id: sha256:def456...
X-Pack-License: LicenseRef-Assay-Enterprise-1.0
Cache-Control: private, max-age=86400

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
| `ETag` | MUST | Strong ETag = digest (for conditional requests) |
| `X-Pack-Digest` | MUST | SHA256 digest (JCS canonical) |
| `X-Pack-Signature` | MUST (commercial) | Ed25519 signature (DSSE PAE) |
| `X-Pack-Key-Id` | MUST (if signed) | SHA256 of SPKI public key |
| `X-Pack-License` | MUST | SPDX identifier (use `LicenseRef-*` for custom) |
| `Cache-Control` | MUST | `private` for authenticated, `public` for open |
| `Digest` | SHOULD | RFC 3230 digest header (for HTTP tooling) |

**Error Responses:**

| Code | Meaning | Body |
|------|---------|------|
| 401 | Unauthorized | `{"error": "authentication_required"}` |
| 403 | Forbidden | `{"error": "license_expired"}` or `{"error": "pack_not_licensed"}` |
| 404 | Not Found | `{"error": "pack_not_found"}` |
| 410 | Gone | `{"error": "security_revocation", "reason": "..."}` |
| 413 | Payload Too Large | `{"error": "pack_exceeds_size_limit"}` |
| 429 | Too Many Requests | `{"error": "rate_limit_exceeded", "retry_after": 60}` |

**410 Gone semantics (NORMATIVE):**

`410` is reserved for **security revocation** (pack pulled due to vulnerability/incident),
NOT for deprecation. Deprecated versions return `200` with `deprecated: true` in metadata.

This ensures reproducible builds don't break due to deprecation; only security issues
warrant hard failure.

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

### 5.2 OIDC Authentication (GitHub Actions)

For CI/CD environments with OIDC:

```yaml
permissions:
  id-token: write

steps:
  - name: Authenticate to Assay Registry
    run: |
      TOKEN=$(curl -s -H "Authorization: bearer $ACTIONS_ID_TOKEN_REQUEST_TOKEN" \
        "$ACTIONS_ID_TOKEN_REQUEST_URL&audience=https://registry.getassay.dev" | jq -r '.value')
      echo "ASSAY_REGISTRY_TOKEN=$TOKEN" >> $GITHUB_ENV
```

**Registry OIDC configuration:**

| Field | Value |
|-------|-------|
| Issuer | `https://token.actions.githubusercontent.com` |
| Audience | `https://registry.getassay.dev` |

**Subject claim patterns (NORMATIVE):**

Registry MUST support flexible subject matching, not hardcoded `refs/heads/main`:

| Pattern | Matches |
|---------|---------|
| `repo:ORG/REPO:*` | Any ref in repo |
| `repo:ORG/REPO:ref:refs/heads/*` | Any branch |
| `repo:ORG/REPO:ref:refs/heads/main` | Specific branch |
| `repo:ORG/REPO:environment:production` | Specific environment |

**Token handling:**

- CLI MUST handle token expiry gracefully (re-fetch on 401)
- CLI SHOULD allow 30s clock skew tolerance
- CLI MUST implement exponential backoff on token fetch failures

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
| Strings, numbers, booleans, null | ✅ Allowed |
| Arrays, objects | ✅ Allowed |
| Duplicate keys | ❌ MUST error |
| Anchors/aliases | ❌ MUST error |
| Tags (!!timestamp, !!binary, etc.) | ❌ MUST error |
| Multi-document | ❌ MUST error |

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

**Signature format**: Ed25519 + DSSE PAE encoding (same as SPEC-Tool-Signing-v1).

**Verification flow:**

```rust
// 1. Extract signature from header
let signature = response.header("X-Pack-Signature")?;
let key_id = response.header("X-Pack-Key-Id")?;

// 2. Verify key is trusted (config or registry-provided trust root)
let public_key = trust_store.get_key(key_id)?;

// 3. Verify signature over canonical content
let canonical = jcs_canonicalize(parse_yaml_strict(content)?)?;
verify_dsse_pae(public_key, &canonical, signature)?;
```

**Trust model:**

| Source | Trust |
|--------|-------|
| Registry-provided key | Trusted if TLS + registry in allowlist |
| Config-provided key | Trusted explicitly |
| Unknown key | MUST reject for commercial packs |

### 6.4 Keyless Signing (Future)

For OIDC-based keyless signing (Sigstore/Fulcio model):

```http
X-Pack-Signature: <signature>
X-Pack-Certificate: <base64-cert>
X-Pack-Transparency-Log: <rekor-entry-url>
```

This is planned for v1.1 and aligns with the attestation positioning in ADR-018.

---

## 7. Caching

### 7.1 Local Cache

```
~/.assay/cache/packs/{name}/{version}/pack.yaml
~/.assay/cache/packs/{name}/{version}/metadata.json
```

**metadata.json:**

```json
{
  "fetched_at": "2026-01-29T10:00:00Z",
  "digest": "sha256:abc123...",
  "expires_at": "2026-01-30T10:00:00Z"
}
```

### 7.2 Cache Invalidation

| Scenario | Behavior |
|----------|----------|
| Cache hit, not expired | Use cached |
| Cache hit, expired | Re-fetch, compare digest |
| Cache miss | Fetch and cache |
| `--no-cache` flag | Always fetch |

### 7.3 Cache TTL

Default: 24 hours (86400 seconds), overridable via `Cache-Control` header.

**Cache-Control requirements:**

| Pack Type | Cache-Control |
|-----------|---------------|
| Commercial (authenticated) | `private, max-age=86400` |
| Open (unauthenticated) | `public, max-age=86400` |

> **Security note**: Commercial packs MUST use `Cache-Control: private` to prevent
> caching by intermediate proxies that don't understand authorization context.

---

## 8. Lockfile

### 8.1 Purpose

Enterprise pipelines need reproducible builds. The lockfile captures resolved pack
references with digests.

### 8.2 Lockfile Format

**Filename**: `assay.packs.lock` (or `assay.lock` with `[packs]` section)

```yaml
# assay.packs.lock
# DO NOT EDIT - Generated by assay pack lock
version: 1
generated_at: "2026-01-29T10:00:00Z"
generated_by: "assay-cli/2.12.0"

packs:
  - name: eu-ai-act-pro
    version: "1.2.0"
    digest: sha256:abc123...
    source: registry
    fetched_at: "2026-01-29T10:00:00Z"
    signature_key_id: sha256:def456...

  - name: eu-ai-act-baseline
    version: "1.0.0"
    digest: sha256:789xyz...
    source: bundled
```

### 8.3 CLI Commands

```bash
# Generate/update lockfile from current packs
assay pack lock

# Verify current packs match lockfile
assay pack lock --verify

# CI mode: fail if lockfile outdated
assay pack lock --check
```

### 8.4 CI Integration

```yaml
steps:
  - name: Verify pack lockfile
    run: assay pack lock --check

  - name: Lint with locked packs
    run: assay evidence lint --pack eu-ai-act-pro@1.2.0 bundle.tar.gz
```

When `assay.packs.lock` exists, CLI SHOULD warn if fetched digest differs from locked digest.

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

### 9.1 User-Facing Errors

| Error | Message |
|-------|---------|
| Not found | `Pack 'eu-ai-act-pro@1.2.0' not found. Check pack name and version.` |
| Auth required | `Pack 'eu-ai-act-pro' requires authentication. Set ASSAY_REGISTRY_TOKEN or configure OIDC.` |
| Not licensed | `Pack 'eu-ai-act-pro' is not included in your license. Contact sales@getassay.dev` |
| Digest mismatch | `Pack integrity check failed. Expected sha256:abc..., got sha256:def...` |
| Deprecated | `Pack 'eu-ai-act-pro@1.1.0' is deprecated. Use @1.2.0 instead.` |

### 9.2 GitHub Action Error

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

### 10.1 Config Commands

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

### 10.2 Fetch Command (Optional)

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
# OCI pull
assay pack pull oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0

# Verify cosign signature
assay pack verify oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0
```

**Media types:**

| Type | Media Type |
|------|------------|
| Pack manifest | `application/vnd.assay.pack.v1+json` |
| Pack content | `application/vnd.assay.pack.content.v1+yaml` |

### 13.3 Coexistence

HTTP registry and OCI distribution will coexist:

```bash
# HTTP registry (default)
--pack eu-ai-act-pro@1.2.0

# OCI registry (explicit)
--pack oci://ghcr.io/assay/packs/eu-ai-act-pro:1.2.0
```

---

## 14. Implementation Checklist

### Phase 1 (v1.0)

- [ ] Pack resolution order in CLI
- [ ] Registry client with token auth
- [ ] OIDC authentication support
- [ ] Strict YAML parsing (reject anchors, duplicates, tags)
- [ ] Digest verification (JCS canonical)
- [ ] Signature verification (MUST for commercial)
- [ ] Local caching with TTL + ETag/304
- [ ] `assay config set registry.*` commands
- [ ] BYOS pack fetch (S3/GCS/Azure)
- [ ] Error messages per §10
- [ ] Rate limit handling (429 + Retry-After)

### Phase 2 (v1.1)

- [ ] Lockfile support (`assay.packs.lock`)
- [ ] Keyless signing (Sigstore/Fulcio)
- [ ] OCI distribution support
- [ ] Organization namespaces

---

## 15. References

### Normative

- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)
- [RFC 8785 - JCS](https://datatracker.ietf.org/doc/html/rfc8785)

### Informative

- [Sigstore](https://sigstore.dev) — Keyless signing
- [ORAS](https://oras.land) — OCI Registry As Storage
- [OPA Bundle Distribution](https://www.openpolicyagent.org/docs/latest/management-bundles/) — Similar pattern
- [TUF](https://theupdateframework.io) — Update security framework
