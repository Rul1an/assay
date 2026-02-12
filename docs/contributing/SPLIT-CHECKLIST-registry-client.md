# Registry client split — checklist & grep-gates

Completed: client.rs → client/ (mod, http, helpers). See PR #302.

## Leak-free contract: mod.rs

**Expect: 0 matches in client/mod.rs (production code, exclude `#[cfg(test)]` blocks).**

```bash
rg "StatusCode|\.status\(|\.as_u16\(|401|404|410|429|RETRY_AFTER|IF_NONE_MATCH" crates/assay-registry/src/client/mod.rs
rg "RegistryError::NotFound|RegistryError::RateLimited|RegistryError::Revoked|RegistryError::Unauthorized" crates/assay-registry/src/client/mod.rs
```

Status mapping lives only in `client/http.rs`. `mod.rs` uses `PackOutcome` / `SignatureOutcome` from http.rs.

## Module layout

| File | Responsibility |
|------|----------------|
| `client/mod.rs` | Public API, `RegistryClient`, no status code handling |
| `client/http.rs` | HTTP layer, `PackOutcome`, `SignatureOutcome`, status mapping, retry |
| `client/helpers.rs` | `parse_pack_url`, `parse_revocation_body`, `compute_digest` |
