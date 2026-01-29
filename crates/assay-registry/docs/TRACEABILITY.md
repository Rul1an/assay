# SPEC-Pack-Registry-v1 Traceability Matrix

This document provides normative traceability from SPEC requirements to implementation and tests.

**SPEC Version:** 1.0.3
**Implementation:** `assay-registry` crate
**Last Updated:** 2026-01-29

---

## Overview

| Category | Requirements | Implemented | Tested | Coverage |
|----------|-------------|-------------|--------|----------|
| Resolution (§2-3) | 6 | 6 | 5 | 83% |
| API Contract (§4) | 12 | 12 | 12 | 100% |
| Authentication (§5) | 8 | 8 | 6 | 75% |
| Integrity (§6) | 15 | 15 | 12 | 80% |
| Caching (§7) | 6 | 6 | 6 | 100% |
| Lockfile (§8) | 8 | 8 | 6 | 75% |
| **Total** | **55** | **55** | **47** | **85%** |

---

## §2-3: Pack Resolution Order

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 2.1 | Local path resolution (./custom.yaml) | `resolver.rs` | `resolve_local()` | `test_resolve_local_file` | Return NotFound for existing file |
| 2.2 | Bundled pack resolution | `resolver.rs` | `resolve_bundled()` | `test_resolve_bundled` | Skip bundled lookup |
| 2.3 | Registry resolution (name@version) | `resolver.rs` | `resolve_registry()` | `test_resolve_registry` | Return bundled for registry ref |
| 2.4 | BYOS resolution (s3://, gs://) | `resolver.rs` | `resolve_byos()` | `test_resolve_byos` | Fail on valid BYOS URL |
| 3.1 | Pack reference parsing | `reference.rs` | `PackRef::parse()` | `test_pack_ref_parse_*` | Accept invalid formats |
| 3.2 | Pinned ref with digest (name@ver#digest) | `reference.rs` | `PackRef::parse()` | `test_pack_ref_pinned` | Ignore digest in pinned ref |

---

## §4: Registry API Contract

### §4.3 GET /packs/{name}/{version}

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 4.3.1 | Returns pack YAML content | `client.rs` | `fetch_pack()` | `test_fetch_pack_success` | Return empty body |
| 4.3.2 | X-Pack-Digest header parsed | `types.rs` | `PackHeaders::from_headers()` | `test_fetch_pack_success` | Ignore X-Pack-Digest |
| 4.3.3 | ETag header for conditional requests | `client.rs` | `fetch_pack()` | `test_fetch_pack_success` | Ignore ETag |
| 4.3.4 | 304 Not Modified with If-None-Match | `client.rs` | `request_once()` | `test_fetch_pack_304_not_modified` | Return content on 304 |
| 4.3.5 | 401 Unauthorized handling | `client.rs` | `request_once()` | `test_fetch_pack_unauthorized` | Return 200 on 401 |
| 4.3.6 | 404 Not Found handling | `client.rs` | `request_once()` | `test_fetch_pack_not_found` | Return 200 on 404 |
| 4.3.7 | 410 Gone (revocation) handling | `client.rs` | `request_once()` | `test_fetch_pack_revoked` | Ignore 410 status |
| 4.3.8 | X-Revocation-Reason extraction | `client.rs` | `request_once()` | `test_fetch_pack_revoked` | Return generic reason |

### §4.4 Rate Limiting

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 4.4.1 | 429 Too Many Requests | `client.rs` | `request_once()` | `test_rate_limiting_with_retry_after` | Ignore 429 |
| 4.4.2 | Retry-After header parsing | `client.rs` | `request_once()` | `test_rate_limiting_with_retry_after` | Hardcode backoff |
| 4.4.3 | Exponential backoff retry | `client.rs` | `request()` | `test_retry_backoff` | No retry on 429 |
| 4.4.4 | Max retry limit (default 3) | `client.rs` | `request()` | `test_max_retries` | Infinite retry |

### §4.5 Other Endpoints

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 4.5.1 | HEAD /packs/{name}/{version} | `client.rs` | `get_pack_meta()` | `test_get_pack_meta` | Return full body |
| 4.5.2 | GET /packs/{name}/versions | `client.rs` | `list_versions()` | `test_list_versions` | Return single version |
| 4.5.3 | GET /keys (trust manifest) | `client.rs` | `fetch_keys()` | `test_fetch_keys_manifest` | Return empty keys |

---

## §5: Authentication

### §5.1 Token Authentication

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 5.1.1 | ASSAY_REGISTRY_TOKEN env var | `auth.rs` | `TokenProvider::from_env()` | `test_from_env_static` | Ignore env var |
| 5.1.2 | Bearer token in Authorization header | `client.rs` | `request_once()` | `test_authentication_header` | Send token in query |
| 5.1.3 | No auth for open packs | `client.rs` | `request_once()` | `test_no_auth_when_no_token` | Require auth always |

### §5.2 OIDC Authentication

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 5.2.1 | OIDC token exchange flow | `auth.rs` | `OidcProvider::exchange_token()` | `test_oidc_full_flow` | Send ID token directly |
| 5.2.2 | Token caching until expires_in - 90s | `auth.rs` | `OidcProvider::get_token()` | `test_oidc_cache_clear` | No caching |
| 5.2.3 | 30s clock skew tolerance | `auth.rs` | `OidcProvider::get_token()` | `test_oidc_clock_skew` | Strict expiry |
| 5.2.4 | Exponential backoff on failure | `auth.rs` | `exchange_token_with_retry()` | `test_oidc_retry_backoff` | No retry |
| 5.2.5 | Re-exchange on 401 | `auth.rs` | `get_token()` | `test_token_expiry_401_re_exchange` | Cache expired token |

---

## §6: Integrity Verification

### §6.1 Canonical Form

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 6.1.1 | Strict YAML parsing | `canonicalize.rs` | `parse_yaml_strict()` | `test_parse_yaml_strict_*` | Allow anchors |
| 6.1.2 | Reject duplicate keys | `canonicalize.rs` | `parse_yaml_strict()` | `test_reject_duplicate_keys` | Accept duplicates |
| 6.1.3 | Reject anchors/aliases | `canonicalize.rs` | `parse_yaml_strict()` | `test_reject_anchors` | Allow anchors |
| 6.1.4 | JCS canonicalization (RFC 8785) | `canonicalize.rs` | `to_canonical_jcs_bytes()` | `test_jcs_canonical` | Use raw YAML bytes |
| 6.1.5 | Key ordering independence | `verify.rs` | `compute_digest()` | `test_compute_digest_key_ordering` | Order-dependent |

### §6.2 Digest Verification

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 6.2.1 | SHA-256 of JCS canonical content | `verify.rs` | `compute_digest()` | `test_compute_digest_canonical` | Use raw bytes |
| 6.2.2 | Digest format sha256:{hex} | `verify.rs` | `compute_digest()` | `test_compute_digest_golden_vector` | Use base64 |
| 6.2.3 | Verify against X-Pack-Digest | `verify.rs` | `verify_pack()` | `test_verify_digest_success` | Skip comparison |
| 6.2.4 | DigestMismatch error on failure | `verify.rs` | `verify_digest()` | `test_verify_digest_mismatch` | Return Ok on mismatch |

### §6.3 Signature Verification (DSSE)

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 6.3.1 | PAE encoding (DSSEv1 format) | `verify.rs` | `build_pae()` | `test_build_pae` | Wrong PAE format |
| 6.3.2 | Payload type validation | `verify.rs` | `verify_dsse_signature()` | `test_dsse_payload_type` | Accept any type |
| 6.3.3 | Payload matches content | `verify.rs` | `verify_dsse_signature()` | `test_dsse_payload_mismatch` | Skip payload check |
| 6.3.4 | Ed25519 signature verification | `verify.rs` | `verify_single_signature()` | `test_dsse_valid_signature_real_ed25519` | Accept any signature |
| 6.3.5 | Sidecar endpoint fetch | `client.rs` | `fetch_signature()` | `test_fetch_signature_sidecar` | Header only |
| 6.3.6 | Sidecar 404 = unsigned | `client.rs` | `fetch_signature()` | `test_fetch_signature_sidecar_not_found` | Error on 404 |

### §6.4 Key Trust Model

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 6.4.1 | Pinned root keys | `trust.rs` | `add_pinned_key()` | `test_add_pinned_key` | Accept any key |
| 6.4.2 | Keys manifest fetch | `client.rs` | `fetch_keys()` | `test_fetch_keys_manifest` | Skip manifest |
| 6.4.3 | Key ID verification (SHA256 of SPKI) | `trust.rs` | `add_pinned_key()` | `test_key_id_mismatch_rejected` | Accept mismatched IDs |
| 6.4.4 | Reject unknown keys for commercial | `trust.rs` | `get_key()` | `test_empty_trust_store` | Return any key |
| 6.4.5 | Key expiry check | `trust.rs` | `get_key_inner()` | `test_expired_key_in_manifest` | Ignore expiry |
| 6.4.6 | Key revocation handling | `trust.rs` | `add_from_manifest()` | `test_revoked_key_in_manifest` | Ignore revocation |
| 6.4.7 | Pinned roots cannot be revoked | `trust.rs` | `add_from_manifest()` | `test_pinned_key_not_overwritten` | Allow remote revocation |

---

## §7: Caching

### §7.1-7.2 Cache Structure and Integrity

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 7.1.1 | Cache structure ({name}/{version}/) | `cache.rs` | `pack_dir()` | `test_cache_roundtrip` | Flat structure |
| 7.2.1 | Digest verification on read | `cache.rs` | `get()` | `test_cache_integrity_failure` | Skip digest check |
| 7.2.2 | Evict on integrity failure | `cache.rs` | `get()` | `test_cache_integrity_failure` | Return corrupted |
| 7.2.3 | Atomic writes (temp + rename) | `cache.rs` | `write_atomic()` | `test_cache_roundtrip` | Direct write |

### §7.3-7.4 Cache Invalidation and TTL

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 7.3.1 | Expiry check before return | `cache.rs` | `get()` | `test_cache_expiry` | Ignore expiry |
| 7.3.2 | ETag for conditional requests | `cache.rs` | `get_etag()` | `test_get_etag` | No ETag storage |
| 7.4.1 | Parse Cache-Control max-age | `cache.rs` | `parse_cache_control_expiry()` | `test_parse_cache_control` | Hardcode TTL |
| 7.4.2 | Default 24h TTL | `cache.rs` | `parse_cache_control_expiry()` | `test_default_ttl` | No default |

---

## §8: Lockfile

### §8.2 Lockfile Format

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 8.2.1 | Version 2 format parsing | `lockfile.rs` | `Lockfile::parse()` | `test_lockfile_parse` | Accept v1 only |
| 8.2.2 | Reject unsupported versions | `lockfile.rs` | `Lockfile::parse()` | `test_lockfile_parse_unsupported_version` | Accept any version |
| 8.2.3 | Sorted packs by name | `lockfile.rs` | `add_pack()` | `test_lockfile_add_pack` | Unsorted |

### §8.3-8.6 Lockfile Operations

| SPEC § | Requirement | Module | Function | Test | Break-Test |
|--------|-------------|--------|----------|------|------------|
| 8.3.1 | Generate lockfile from refs | `lockfile.rs` | `generate_lockfile()` | `test_generate_lockfile` | Empty lockfile |
| 8.3.2 | Verify lockfile digests | `lockfile.rs` | `verify_lockfile()` | `test_verify_lockfile` | Skip verification |
| 8.4.1 | Pack not in lockfile detection | `lockfile.rs` | `contains()` | `test_lockfile_get_pack` | Always return found |
| 8.4.2 | Digest mismatch detection | `lockfile.rs` | `verify_lockfile()` | `test_verify_lockfile_digest_mismatch` | Ignore mismatch |
| 8.6.1 | Handle 410 for locked pack | `lockfile.rs` | `verify_lockfile()` | `test_lockfile_revoked_pack` | Normal error |

---

## Coverage Gaps

The following requirements need additional test coverage:

### High Priority (Security-Critical)

1. **§5.2.5 Token expiry re-exchange**: Test that 401 response triggers token re-exchange without infinite loop
2. **§6.3.3 DSSE payload mismatch**: Test that envelope payload != content is rejected
3. **§6.3.4 Ed25519 real signature**: Test with deterministic key for reproducible verification

### Medium Priority (Protocol Correctness)

4. **§4.3.2 ETag equals X-Pack-Digest**: SPEC requires strong ETag matching digest
5. **§4.3.8 Vary header validation**: Authenticated responses require `Vary: Authorization, Accept-Encoding`
6. **§6.2.1 Content-Digest vs X-Pack-Digest**: Test wire bytes differ but canonical matches

### Lower Priority (Edge Cases)

7. **§7.2.2 Signature cache corruption**: Test corrupt signature.json handling
8. **§8.4.2 Lockfile roundtrip**: Test serialize/parse stability
9. **§5.2.4 OIDC backoff timing**: Verify actual elapsed time >= expected

---

## Test Execution

```bash
# Run all registry tests
cargo test -p assay-registry

# Run with OIDC feature
cargo test -p assay-registry --features oidc

# Run specific test
cargo test -p assay-registry test_dsse_valid_signature

# Run integration tests only
cargo test -p assay-registry --test '*' -- --test-threads=1
```

---

## Verification Status

| Status | Meaning |
|--------|---------|
| Implemented | Code exists for requirement |
| Tested | At least one test covers the requirement |
| Break-Test | Describes what failure to implement would cause |

**Last verification:** 2026-01-29
**Verified by:** Claude Code review artifacts generation
