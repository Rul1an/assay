# SPEC-Pack-Registry-v1

**Version:** 1.0.0
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
- Integrity verification (digest)
- Caching behavior

### 1.2 Out of Scope

- Pack content/rules (see SPEC-Pack-Engine-v1)
- Registry hosting implementation
- Billing/licensing enforcement

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
pack_ref := local_path | bundled_name | registry_ref | byos_url

local_path    := "./" path ".yaml"
bundled_name  := identifier                    # e.g., "eu-ai-act-baseline"
registry_ref  := identifier "@" version        # e.g., "eu-ai-act-pro@1.2.0"
byos_url      := scheme "://" path ".yaml"     # e.g., "s3://bucket/packs/custom.yaml"
```

**Version requirement**: Registry refs MUST include version. `@latest` is NOT supported
for reproducibility.

---

## 4. Registry API Contract

### 4.1 Base URL

```
Default: https://registry.getassay.dev/v1
Override: ASSAY_REGISTRY_URL environment variable
```

### 4.2 Endpoints

#### GET /packs/{name}/{version}

Fetch pack content.

**Request:**

```http
GET /packs/eu-ai-act-pro/1.2.0 HTTP/1.1
Host: registry.getassay.dev
Authorization: Bearer <token>
Accept: application/x-yaml
```

**Response (200 OK):**

```http
HTTP/1.1 200 OK
Content-Type: application/x-yaml
X-Pack-Digest: sha256:abc123...
X-Pack-License: Assay-Enterprise-1.0
Cache-Control: public, max-age=86400

name: eu-ai-act-pro
version: "1.2.0"
kind: compliance
...
```

**Response Headers (REQUIRED):**

| Header | Description |
|--------|-------------|
| `X-Pack-Digest` | SHA256 digest of pack content (JCS canonical) |
| `X-Pack-License` | SPDX license identifier |
| `Cache-Control` | Caching directive |

**Error Responses:**

| Code | Meaning | Body |
|------|---------|------|
| 401 | Unauthorized | `{"error": "authentication_required"}` |
| 403 | Forbidden | `{"error": "license_expired"}` or `{"error": "pack_not_licensed"}` |
| 404 | Not Found | `{"error": "pack_not_found"}` |
| 410 | Gone | `{"error": "version_deprecated"}` |

#### GET /packs/{name}/versions

List available versions.

**Response (200 OK):**

```json
{
  "name": "eu-ai-act-pro",
  "versions": [
    {"version": "1.2.0", "released": "2026-01-15", "deprecated": false},
    {"version": "1.1.0", "released": "2025-11-01", "deprecated": true}
  ],
  "latest": "1.2.0"
}
```

#### GET /packs

List available packs (for discovery).

**Response (200 OK):**

```json
{
  "packs": [
    {"name": "eu-ai-act-pro", "latest": "1.2.0", "license": "commercial"},
    {"name": "soc2-pro", "latest": "1.0.0", "license": "commercial"}
  ]
}
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
| Subject claim | `repo:ORG/REPO:ref:refs/heads/main` |

### 5.3 No Authentication (Open Packs)

Open packs MAY be served without authentication for convenience:

```http
GET /packs/eu-ai-act-baseline/1.0.0 HTTP/1.1
# No Authorization header required
```

---

## 6. Integrity Verification

### 6.1 Digest Verification (NORMATIVE)

After fetching, CLI MUST verify digest:

```rust
let fetched_content = fetch_pack(url)?;
let computed_digest = sha256(jcs_canonicalize(parse_yaml(fetched_content)?));
let expected_digest = response.header("X-Pack-Digest");

if computed_digest != expected_digest {
    return Err(PackIntegrityError::DigestMismatch);
}
```

### 6.2 Signature Verification (OPTIONAL)

Packs MAY include `x-assay-sig` field for Ed25519 signature verification
(same as tool signing, see SPEC-Tool-Signing-v1).

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

---

## 8. BYOS Pack Storage

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

## 9. Error Messages

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

## 10. CLI Implementation

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

## 11. Security Considerations

### 11.1 Token Security

- Tokens SHOULD be short-lived (< 24h for CI)
- Tokens MUST NOT be logged
- OIDC preferred over long-lived tokens

### 11.2 MITM Protection

- Registry MUST use HTTPS
- CLI MUST verify TLS certificates
- Digest verification provides content integrity

### 11.3 Supply Chain

- Pack digests are computed from JCS-canonical content
- Unknown YAML fields cause validation failure (no injection via ignored fields)
- Signed packs provide author verification

---

## 12. Implementation Checklist

- [ ] Pack resolution order in CLI
- [ ] Registry client with token auth
- [ ] OIDC authentication support
- [ ] Digest verification
- [ ] Local caching with TTL
- [ ] `assay config set registry.*` commands
- [ ] BYOS pack fetch (S3/GCS/Azure)
- [ ] Error messages per §9
- [ ] GitHub Action documentation

---

## 13. References

- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [SPEC-Tool-Signing-v1](./SPEC-Tool-Signing-v1.md)
- [RFC 8785 - JCS](https://datatracker.ietf.org/doc/html/rfc8785)
