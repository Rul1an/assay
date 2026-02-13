# Canonicalize split — checklist & grep-gates

Completed: canonicalize.rs → canonicalize/ (mod, yaml, json, digest, errors, tests). See PR #308.

## Leak-free contract

**mod.rs:** No YAML/hashing internals. Expect 0 matches (tests live in tests.rs).

```bash
rg "serde_yaml::|use serde_yaml|sha2::|Sha256::|hex::|Digest::" crates/assay-registry/src/canonicalize/mod.rs
# Expect: 0
```

**yaml.rs:** No IO. Expect 0.

```bash
rg "reqwest|tokio::fs|std::fs|Url|StatusCode" crates/assay-registry/src/canonicalize/yaml.rs
# Expect: 0
```

**digest.rs:** No YAML/JCS. Expect 0.

```bash
rg "serde_yaml|jcs|rfc8785" crates/assay-registry/src/canonicalize/digest.rs
# Expect: 0
```

## Module layout

| File | Responsibility |
|------|----------------|
| `canonicalize/mod.rs` | Façade, orchestration (parse → jcs → digest), re-exports |
| `canonicalize/mod.rs` | No serde_yaml, sha2, hex in implementation |
| `canonicalize/yaml.rs` | parse_yaml_strict, pre_scan_yaml, yaml_to_json |
| `canonicalize/json.rs` | to_canonical_jcs_bytes (JCS/RFC 8785) |
| `canonicalize/digest.rs` | sha256_prefixed |
| `canonicalize/errors.rs` | CanonicalizeError, MAX_* constants |
| `canonicalize/tests.rs` | Behavior freeze tests (separate from mod for grep-gates) |

## Digest flow

`compute_canonical_digest` → `parse_yaml_strict` → `to_canonical_jcs_bytes` → `digest::sha256_prefixed(&bytes)`.

Digest always over JCS bytes (not string). Format: `sha256:{lowercase_hex}`.

## ParseError machine-readable reasons

ParseError uses `reason=<code>: <message>` for stable matching (e.g. `reason=merge_key_not_allowed`).

## Duplicate key equality

- Pre-scan: token-level, no Unicode normalization
- serde_yaml flow: parser rejects
- yaml_to_json: Value key equality, no NFC/NFKC

## Merge gates

- Golden digest parity: `test_golden_vector_basic_pack`
- Stability: `test_jcs_key_ordering`, `test_whitespace_normalization`
- YAML gotchas: merge keys, tags, binary, duplicate keys — all rejected
