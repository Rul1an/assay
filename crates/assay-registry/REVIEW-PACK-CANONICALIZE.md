# Canonicalize split – review pack

**PR:** [#308](https://github.com/Rul1an/assay/pull/308) refactor(registry): split canonicalize.rs into module
**SPLIT-PLAN:** §4.2 canonicalize.rs (1155 LOC) → canonicalize/ (mod, yaml, json, digest, errors, tests)
**Checklist:** [docs/contributing/SPLIT-CHECKLIST-canonicalize.md](../../docs/contributing/SPLIT-CHECKLIST-canonicalize.md)

---

## 1. Copy-paste review checklist

```bash
# === Required build/test ===
cargo test -p assay-registry
cargo clippy -p assay-registry --all-targets -- -D warnings

# === Forbidden-knowledge gates (code usage only; tests in tests.rs) ===
# mod.rs: no YAML/hashing internals — expect 0 (matches code, not doc comments)
rg "serde_yaml::|use serde_yaml|sha2::|Sha256::|hex::|Digest::" crates/assay-registry/src/canonicalize/mod.rs
# Expect: 0

# mod.rs: no transitive heavy deps in public API
rg "serde_yaml::|sha2::" crates/assay-registry/src/canonicalize/mod.rs
# Expect: 0

# yaml.rs: no IO — expect 0
rg "reqwest|tokio::fs|std::fs|Url|StatusCode" crates/assay-registry/src/canonicalize/yaml.rs
# Expect: 0

# digest.rs: no YAML/JCS — expect 0
rg "serde_yaml|jcs|rfc8785" crates/assay-registry/src/canonicalize/digest.rs
# Expect: 0

# === API path smoke check ===
rg "pub use|pub fn|pub const" crates/assay-registry/src/canonicalize/mod.rs
```

---

## 2. Target structure & responsibilities

| File | Responsibility |
|------|----------------|
| `mod.rs` | Façade, orchestration (parse → jcs → digest), re-exports |
| `tests.rs` | Behavior freeze tests (separate file for robust grep-gates) |
| `errors.rs` | CanonicalizeError, CanonicalizeResult, MAX_* constants |
| `yaml.rs` | parse_yaml_strict, pre_scan_yaml, yaml_to_json, extract_yaml_key, is_inside_quotes |
| `json.rs` | to_canonical_jcs_bytes (JCS/RFC 8785) |
| `digest.rs` | sha256_prefixed (bytes → "sha256:{hex}") |

---

## 3. Leak-free contract

| mod.rs | yaml.rs | json.rs | digest.rs |
|--------|---------|---------|-----------|
| No serde_yaml, Sha256, hex | May serde_yaml, Value | May serde_jcs | May sha2, hex |
| Orchestration only | No reqwest, fs | No sha2, serde_yaml | No serde_yaml, jcs |

---

## 4. Digest flow (canonical bytes only)

`compute_canonical_digest` → `parse_yaml_strict` → `to_canonical_jcs_bytes(&json)` → `digest::sha256_prefixed(&jcs_bytes)`.

- Digest always over **JCS bytes** (not string; no encoding/line-ending drift).
- Format: `sha256:{lowercase_hex}` (format!("sha256:{:x}", hash)).
- No `format!("{:?}", bytes)` or `.to_string()` on JSON value as hash input.

---

## 5. Public API (unchanged)

| Symbol | Re-export path |
|--------|----------------|
| `parse_yaml_strict` | `canonicalize::parse_yaml_strict` |
| `to_canonical_jcs_bytes` | `canonicalize::to_canonical_jcs_bytes` |
| `compute_canonical_digest` | `canonicalize::compute_canonical_digest` |
| `compute_canonical_digest_result` | `canonicalize::compute_canonical_digest_result` |
| `CanonicalizeError`, `CanonicalizeResult` | `canonicalize::` |
| `MAX_DEPTH`, `MAX_KEYS_PER_MAPPING`, etc. | `canonicalize::` |

**Consumers:** `lib.rs` (re-exports), `client/mod.rs`, `verify.rs`, `digest.rs` — path unchanged.

---

## 6. ParseError machine-readable reasons

ParseError messages use `reason=<code>: <human message>` for stable matching:

- `reason=merge_key_not_allowed` — YAML merge keys (`<<`) rejected

## 7. YAML gotchas (covered)

| Gotcha | Status |
|--------|--------|
| Merge keys (`<<`) | Rejected (pre_scan + yaml_to_json) — `test_reject_merge_key` |
| Tags (!!str, !!int, !!timestamp, !!binary) | Rejected — `test_reject_tag_*` |
| Anchors/aliases | Rejected — `test_reject_anchor`, `test_reject_alias` |
| Duplicate keys | Pre-scan (block) + serde_yaml (flow) + yaml_to_json (Mapping) |
| CRLF/tabs in indentation | YAML spec: tabs invalid in indentation; serde_yaml handles |

---

## 8. Duplicate key equality (documented)

- **Pre-scan**: Token-level, raw YAML lines. No Unicode normalization.
- **serde_yaml flow**: Parser rejects flow duplicates.
- **yaml_to_json Mapping**: `serde_yaml::Value` key equality. No NFC/NFKC (like json_strict).

## 9. Behavior freeze tests

| Category | Key tests |
|----------|-----------|
| Golden digest | `test_golden_vector_basic_pack` |
| Digest over bytes | `test_digest_over_jcs_bytes_not_string` (non-ASCII, guards against .to_string() regress) |
| Key ordering | `test_jcs_key_ordering`, `test_whitespace_normalization` |
| Rejection | `test_reject_anchor`, `test_reject_float`, `test_reject_merge_key`, `test_reject_duplicate_keys_*` |
| DoS limits | `test_reject_deep_nesting`, `test_reject_input_too_large` |
| Edge cases | `test_ampersand_in_string_allowed`, `test_list_items_same_key_allowed` |

**Verification:** 171 unit tests + 26 integration tests pass.

---

## Error mapping & SerializeError

- **RegistryError**: CanonicalizeError maps to `InvalidResponse { message: "canonicalization failed (pack invalid/unsupported): {err}" }` — explicit that pack is invalid/unsupported, not just "registry sent bad data".
- **SerializeError**: Preserves full `e.to_string()`. Consider `#[source]` for error chaining if Clone can be relaxed.

## 10. Merge gates

- [ ] Golden digest parity: `test_golden_vector_basic_pack` digest unchanged
- [ ] Stability: key order / whitespace variants → same digest
- [ ] All tests pass
- [ ] No import churn in consumers
- [ ] Error contract stable (CanonicalizeError variants unchanged)

---

## 11. Minimale 3-stukken samenvatting

1. **mod.rs** – orchestration (parse → jcs → digest), re-exports; tests in tests.rs
2. **yaml.rs** – parsing + validation (pre_scan, yaml_to_json)
3. **digest.rs** – sha256_prefixed

**API paths unchanged.** `use crate::canonicalize::parse_yaml_strict` werkt ongewijzigd.
