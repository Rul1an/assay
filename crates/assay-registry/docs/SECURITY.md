# Security Review: assay-registry

This document describes the security model, threat analysis, and mitigations for the pack registry client.

**SPEC:** SPEC-Pack-Registry-v1.0.3
**Crate:** `assay-registry`
**Last Updated:** 2026-01-29

---

## 1. Trust Model

### 1.1 Trust Bootstrap (No TOFU)

The registry client does NOT use Trust On First Use (TOFU). Trust is explicitly bootstrapped:

```
┌─────────────────────────────────────────────────────────────────┐
│                      CLI Binary (Pinned)                        │
│  TrustStore::with_pinned_roots([sha256:root1, sha256:root2])   │
└─────────────────────────────────────────────────────────────────┘
                               │
                               │ 1. Validates manifest signature
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                    Keys Manifest (/keys)                        │
│  {                                                              │
│    "keys": [{ "id": "sha256:pack-signing-key", ... }],         │
│    "signature": "<DSSE signed by pinned root>"                 │
│  }                                                              │
└─────────────────────────────────────────────────────────────────┘
                               │
                               │ 2. Pack signatures verified against manifest keys
                               ▼
┌─────────────────────────────────────────────────────────────────┐
│                     Pack Signature                              │
│  X-Pack-Signature: <DSSE envelope>                             │
│  keyid: sha256:pack-signing-key (from manifest)                │
└─────────────────────────────────────────────────────────────────┘
```

**Key properties:**

| Property | Behavior |
|----------|----------|
| Pinned roots | Compiled into binary, cannot be remotely modified |
| Unknown keys | REJECTED for commercial packs |
| Key expiry | Enforced at verification time |
| Key revocation | Removes key from trust store (except pinned) |

### 1.2 Key Lifecycle

```
┌────────────┐     ┌────────────┐     ┌────────────┐
│  Genesis   │     │  Active    │     │  Rotated   │
│            │────▶│            │────▶│            │
│ not_before │     │ signing    │     │ expires_at │
└────────────┘     └────────────┘     └────────────┘
                                            │
                                            ▼
                                   ┌────────────┐
                                   │  Expired/  │
                                   │  Revoked   │
                                   └────────────┘
```

**Rotation process:**

1. New key added to manifest with future `not_before`
2. Old key gets `expires_at` timestamp
3. CLI fetches manifest (24h TTL), validates against pinned roots
4. Transition period: both keys valid
5. After `expires_at`: old key rejected

**Critical invariant:** Pinned roots CANNOT be remotely revoked. This defends against a compromised registry attempting to brick all clients.

### 1.3 Revocation Policy

| Scenario | CLI Behavior |
|----------|--------------|
| 410 revoked pack | Error with safe version suggestion |
| `--allow-revoked` | Explicit opt-in required (forensics) |
| `ASSAY_ALLOW_REVOKED=forensics` | CI must set explicit env value |
| Key revoked in manifest | Key removed from trust store |
| Pinned root in "revoked" manifest | Key STILL trusted (pinned survives) |

---

## 2. Threat Model

### 2.1 Attacker Capabilities

| Attacker | Capabilities | Defenses |
|----------|------------|----------|
| Network (MITM) | Intercept traffic, modify responses | TLS (rustls), digest verification |
| Compromised Registry | Serve malicious packs | Signature verification, pinned digests |
| Compromised Cache | Modify local files | Digest-on-every-read |
| Supply Chain | Malicious CI PR | Lockfile verification, digest pinning |

### 2.2 Abuse Cases

#### A1: Compromised Registry Serves Malicious Pack

**Attack:** Registry infrastructure compromised, attacker serves pack with malicious rules.

**Scenario:**
```
1. Attacker compromises registry backend
2. Serves pack with X-Pack-Digest matching malicious content
3. CLI fetches and uses malicious rules
```

**Mitigations:**
| Defense | Implementation | Residual Risk |
|---------|----------------|---------------|
| Signature verification | `verify_dsse_signature()` | If signing key also compromised |
| Pinned digests | `PackRef::Pinned { digest }` | None with pinned ref |
| Key rotation | Keys manifest with expiry | Key compromise window |

**Residual risk:** If both registry AND signing key are compromised simultaneously, malicious packs can be served. Defense: pinned refs in lockfile.

#### A2: Man-in-the-Middle Attack

**Attack:** Attacker intercepts network traffic, modifies pack content in transit.

**Scenario:**
```
1. Attacker performs MITM on HTTP(S) connection
2. Modifies pack YAML in transit
3. CLI receives tampered content
```

**Mitigations:**
| Defense | Implementation | Residual Risk |
|---------|----------------|---------------|
| TLS | `reqwest` with `rustls-tls` | None with valid TLS |
| X-Pack-Digest verification | `verify_digest()` | None (digest would mismatch) |
| Certificate pinning | Future (TUF/sigstore) | Current: rely on CA system |

**Residual risk:** None with proper TLS. Even without TLS, digest verification catches modification.

#### A3: Malicious Cache Tampering

**Attack:** Malware or local attacker modifies cached pack files.

**Scenario:**
```
1. Pack cached at ~/.assay/cache/packs/
2. Attacker modifies pack.yaml on disk
3. CLI loads tampered pack
```

**Mitigations:**
| Defense | Implementation | Residual Risk |
|---------|----------------|---------------|
| Digest on every read | `PackCache::get()` line 146 | None |
| Atomic writes | `write_atomic()` | None |
| Signature verification | `CacheEntry.signature` | None (re-verified) |

**Code path:**
```rust
// cache.rs:146 - CRITICAL: Verify digest BEFORE returning
let computed_digest = compute_digest(&content);
if computed_digest != metadata.digest {
    return Err(RegistryError::DigestMismatch { ... });
}
```

**Residual risk:** None. Cache is treated as untrusted.

#### A4: Fork/PR Supply Chain Attack

**Attack:** Malicious PR modifies lockfile to reference attacker-controlled pack.

**Scenario:**
```
1. Attacker forks repo, modifies assay.packs.lock
2. Changes digest to point to malicious pack
3. Reviewer doesn't notice, PR merged
4. CI uses malicious pack
```

**Mitigations:**
| Defense | Implementation | Residual Risk |
|---------|----------------|---------------|
| Lockfile verification | `verify_lockfile()` | Reviewer must verify changes |
| Digest pinning in CI | `--pack name@ver#digest` | None with pinned ref |
| CI digest check | `assay pack lock --check` | Alerts on changes |

**Residual risk:** Reviewer must verify lockfile changes. Recommend: CI job that fails on lockfile modification without explicit approval.

#### A5: Denial of Service via YAML Parsing

**Attack:** Malicious pack with YAML bomb (billion laughs, deep nesting).

**Scenario:**
```yaml
# Billion laughs attack
a: &a ["lol","lol","lol","lol","lol","lol","lol","lol","lol"]
b: &b [*a,*a,*a,*a,*a,*a,*a,*a,*a]
c: &c [*b,*b,*b,*b,*b,*b,*b,*b,*b]
# Exponential expansion
```

**Mitigations:**
| Defense | Implementation | Residual Risk |
|---------|----------------|---------------|
| Reject anchors/aliases | `parse_yaml_strict()` | None |
| Depth limit (50) | `MAX_DEPTH` constant | None |
| String limit (1MB) | `MAX_STRING_LENGTH` | None |
| Key limit (10,000) | `MAX_KEYS_PER_MAPPING` | None |

**Code path:**
```rust
// canonicalize.rs - Strict YAML parsing
pub const MAX_DEPTH: usize = 50;
pub const MAX_STRING_LENGTH: usize = 1_000_000;
pub const MAX_KEYS_PER_MAPPING: usize = 10_000;
```

**Residual risk:** None. YAML parser configured with strict limits.

---

## 3. OIDC Token Exchange

### 3.1 Relation to RFC 8693 (Token Exchange)

The OIDC authentication implements the RFC 8693 Token Exchange pattern:

```
┌────────────┐         ┌────────────┐         ┌────────────┐
│   GitHub   │         │    CLI     │         │  Registry  │
│  Actions   │         │            │         │            │
└─────┬──────┘         └─────┬──────┘         └─────┬──────┘
      │                      │                      │
      │ 1. Request ID token  │                      │
      │ (audience=registry)  │                      │
      │◀─────────────────────│                      │
      │                      │                      │
      │ 2. ID token (JWT)    │                      │
      │─────────────────────▶│                      │
      │                      │                      │
      │                      │ 3. POST /auth/oidc/exchange
      │                      │    grant_type=token-exchange
      │                      │    subject_token=<JWT>
      │                      │─────────────────────▶│
      │                      │                      │
      │                      │ 4. access_token      │
      │                      │    expires_in        │
      │                      │◀─────────────────────│
      │                      │                      │
      │                      │ 5. Bearer <token>    │
      │                      │─────────────────────▶│
```

### 3.2 JWT Claims Handling

**Critical design decision:** JWT claims validation is SERVER-SIDE by design.

| Claim | CLI Handling | Server Handling |
|-------|-------------|-----------------|
| `iss` | Not validated | MUST match `https://token.actions.githubusercontent.com` |
| `aud` | Sets to registry URL | MUST match expected audience |
| `sub` | Not validated | Authorization based on patterns |
| `exp` | Respected via `expires_in` | Validated server-side |

**Why CLI doesn't validate claims:**

1. CLI is not the relying party - registry is
2. Prevents "confused deputy" if CLI validated wrong claims
3. Registry enforces authorization policy centrally

**Subject claim patterns (server-side):**

```
repo:ORG/REPO:*                          # Any ref in repo
repo:ORG/REPO:ref:refs/heads/*           # Any branch
repo:ORG/REPO:ref:refs/heads/main        # Specific branch
repo:ORG/REPO:environment:production     # Specific environment
```

### 3.3 Token Security

| Property | Implementation | Location |
|----------|----------------|----------|
| Cache expiry | `expires_in - 90s` | `auth.rs:215` |
| Clock skew tolerance | 30s buffer + 60s margin | `auth.rs:214` |
| Retry backoff | 1s, 2s, 4s, max 30s | `auth.rs:240` |
| Token logging | **NEVER** logged | All debug output redacts |

**Cache invalidation:**
```rust
// auth.rs:214-216
let buffer = chrono::Duration::seconds(90);
if cached.expires_at > chrono::Utc::now() + buffer {
    return Ok(Some(cached.token.clone()));
}
```

**Backoff implementation:**
```rust
// auth.rs:240-241
let backoff = std::time::Duration::from_secs(1 << retries);
let backoff = backoff.min(std::time::Duration::from_secs(30));
```

---

## 4. Security Boundaries

### 4.1 Data Flow Security Boundaries

```
┌─────────────────────────────────────────────────────────────────┐
│                       UNTRUSTED                                 │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │   Network    │  │  Local Disk  │  │  User Input  │          │
│  │   (TLS)      │  │  (Cache)     │  │  (Pack Ref)  │          │
│  └──────┬───────┘  └──────┬───────┘  └──────┬───────┘          │
└─────────┼─────────────────┼─────────────────┼───────────────────┘
          │                 │                 │
          ▼                 ▼                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                    VALIDATION LAYER                             │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Digest      │  │  Digest      │  │  PackRef     │          │
│  │  Verify      │  │  Verify      │  │  Parse       │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
└─────────────────────────────────────────────────────────────────┘
          │                 │                 │
          ▼                 ▼                 ▼
┌─────────────────────────────────────────────────────────────────┐
│                       TRUSTED                                   │
│  ┌──────────────┐  ┌──────────────┐  ┌──────────────┐          │
│  │  Signature   │  │  Signature   │  │  Resolver    │          │
│  │  Verify      │  │  Verify      │  │              │          │
│  └──────────────┘  └──────────────┘  └──────────────┘          │
└─────────────────────────────────────────────────────────────────┘
```

### 4.2 Verification Checkpoints

| Checkpoint | Function | Failure Mode |
|------------|----------|--------------|
| Network fetch | `verify_digest()` | `DigestMismatch` |
| Cache read | `PackCache::get()` | `DigestMismatch`, evict |
| Signature | `verify_dsse_signature()` | `SignatureInvalid` |
| Key trust | `TrustStore::get_key()` | `KeyNotTrusted` |
| Pack parse | `parse_yaml_strict()` | `CanonicalizeError` |

---

## 5. Recommendations

### 5.1 Deployment Recommendations

1. **Always use lockfiles in CI** - Prevents supply chain attacks
2. **Pin pack digests** - `--pack name@ver#sha256:...`
3. **Use OIDC over static tokens** - Shorter lifetime, no secret rotation
4. **Review lockfile changes** - Treat as security-sensitive

### 5.2 Future Enhancements

| Enhancement | Description | Status |
|-------------|-------------|--------|
| Certificate pinning | Pin registry TLS cert | Planned v1.2 |
| Sigstore/Fulcio | Keyless signing | Planned v1.1 |
| TUF integration | Update framework | Under consideration |
| Transparency log | Pack audit trail | Under consideration |

---

## 6. Security Testing

### 6.1 Covered Scenarios

| Test | Location | Scenario |
|------|----------|----------|
| `test_cache_integrity_failure` | `cache.rs` | Cache tampering detection |
| `test_verify_digest_mismatch` | `verify.rs` | Network tampering detection |
| `test_key_id_mismatch_rejected` | `trust.rs` | Invalid key rejection |
| `test_pinned_key_not_overwritten` | `trust.rs` | Remote revocation attack |
| `test_revoked_key_in_manifest` | `trust.rs` | Key revocation propagation |

### 6.2 Recommended Additional Tests

1. **Signature bypass attempt** - Test that unsigned pack with `allow_unsigned=false` fails
2. **Expired key verification** - Test that key past `expires_at` is rejected at runtime
3. **Token not in debug output** - Test that `Debug` impl redacts tokens
4. **OIDC infinite loop prevention** - Test that persistent 401 doesn't loop forever

---

## Appendix: Error Codes

| Exit Code | Category | Errors |
|-----------|----------|--------|
| 1 | Not Found / Config | `NotFound`, `Config`, `InvalidReference` |
| 2 | Auth | `Unauthorized` |
| 3 | Security (revocation) | `Revoked` |
| 4 | Security (integrity) | `DigestMismatch`, `SignatureInvalid`, `KeyNotTrusted`, `Unsigned` |
| 5 | Network | `RateLimited`, `Network` |
| 6 | Cache/Response | `Cache`, `InvalidResponse` |
| 7 | Lockfile | `Lockfile` |

Exit codes 3-4 are security-relevant and should trigger investigation.
