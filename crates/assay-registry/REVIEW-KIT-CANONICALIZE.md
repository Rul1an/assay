# Canonicalize split – review kit

PR: refactor(registry): split canonicalize.rs into module

SPLIT-PLAN: §4.2 canonicalize.rs (1155 LOC) → canonicalize/ (mod, yaml, json, digest, errors)

---

## 1) Target structure

```
canonicalize/
  mod.rs     # Façade, orchestration, re-exports, tests
  errors.rs  # CanonicalizeError, CanonicalizeResult, MAX_* constants
  yaml.rs    # parse_yaml_strict, pre_scan_yaml, yaml_to_json, helpers
  json.rs    # to_canonical_jcs_bytes (JCS/RFC 8785)
  digest.rs  # sha256_prefixed
```

## 2) Leak-free contract

| mod.rs | yaml.rs | digest.rs |
|--------|---------|-----------|
| No serde_yaml, Sha256, hex | May serde_yaml, Value | May sha2, hex |
| Orchestration only | No reqwest, fs | No serde_yaml, jcs |

## 3) Forbidden grep (mod.rs implementation)

```bash
rg "serde_yaml|Sha256|hex::|sha2::|Digest" crates/assay-registry/src/canonicalize/mod.rs
# Expect: 0 in implementation (comments/tests may match)
```

## 4) Public API unchanged

- `parse_yaml_strict`, `to_canonical_jcs_bytes`, `compute_canonical_digest`, `compute_canonical_digest_result`
- `CanonicalizeError`, `CanonicalizeResult`, `MAX_*`, `MIN_SAFE_INTEGER`

## 5) Merge gates

- Golden digest parity: `test_golden_vector_basic_pack` unchanged
- Stability: `test_jcs_key_ordering`, `test_whitespace_normalization` unchanged
- All 169 unit + 26 integration tests pass
