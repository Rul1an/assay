# json_strict split – diff-proof review kit

PR: refactor(evidence): split json_strict.rs into mod, errors, scan, dupkeys

SPLIT-PLAN: §4.1 json_strict (~970) — quick win. Implementation order step 1.

---

## 1) Public API & semantiek-pariteit (must-have)

### json_strict/mod.rs – public API

```rust
pub fn from_str_strict<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError>
pub fn validate_json_strict(s: &str) -> Result<(), StrictJsonError>

pub enum StrictJsonError {
    DuplicateKey { key, path },
    InvalidUnicodeEscape { position },
    LoneSurrogate { position, codepoint },
    ParseError(serde_json::Error),
    NestingTooDeep { depth },
    TooManyKeys { count },
    StringTooLong { length },
}

pub const MAX_NESTING_DEPTH: usize = 64;
pub const MAX_KEYS_PER_OBJECT: usize = 10_000;
pub const MAX_STRING_LENGTH: usize = 1_048_576;
```

### Semantiek

| Scenario | Expected |
|----------|----------|
| `{"a": 1, "a": 2}` | `Err(DuplicateKey { key: "a", path: "/" })` |
| `{"a": 1, "\u0061": 2}` | `Err(DuplicateKey { key: "a", ... })` (unicode escape = duplicate) |
| `{"key": "\uD800"}` | `Err(LoneSurrogate { .. })` (lone high surrogate) |
| 65 levels nesting | `Err(NestingTooDeep { depth: 65 })` |
| 10_001 keys in object | `Err(TooManyKeys { .. })` |
| String > 1MB decoded | `Err(StringTooLong { .. })` |

### Module layout

| File | Responsibility |
|------|----------------|
| `mod.rs` | Public API, JsonValidator orchestration, value parsing (number/bool/null/array/object) |
| `errors.rs` | StrictJsonError, MAX_* constants |
| `scan.rs` | `parse_json_string()` – unicode escapes, surrogate pairs, standard escapes |
| `dupkeys.rs` | ObjectKeyTracker – per-object key set, DuplicateKey + TooManyKeys |

---

## 2) Leak-free contract bewijs (must-have)

### mod.rs – no scan/unicode logic, no direct HashSet

```bash
rg "0xD800|0xDC00|0xDBFF|0xDFFF|surrogate|from_u32|char::from" crates/assay-evidence/src/json_strict/mod.rs
```

**Expect:** 0 matches. Unicode/surrogate logic lives only in `scan.rs`.

```bash
rg "HashSet::|\.insert\(|keys\.len\(|MAX_KEYS_PER_OBJECT|MAX_STRING_LENGTH" crates/assay-evidence/src/json_strict/mod.rs
```

**Expect:** 0 matches in production code. Duplicate/key-count logic lives only in `dupkeys.rs`. (Limits used in mod.rs only for nesting depth.)

### scan.rs – no duplicate-key knowledge

```bash
rg "DuplicateKey|push_key|ObjectKeyTracker|object_stack" crates/assay-evidence/src/json_strict/scan.rs
```

**Expect:** 0 matches. Scan only parses strings; no object-structure awareness.

### dupkeys.rs – no parsing

```bash
rg "chars\.|next_char|peek_char|parse_json|CharIndices" crates/assay-evidence/src/json_strict/dupkeys.rs
```

**Expect:** 0 matches. Dupkeys only tracks keys; no char-level parsing.

---

## 3) Consumer imports unchanged (must-have)

```bash
rg "json_strict::|validate_json_strict|from_str_strict|StrictJsonError" crates/
```

**Consumers:** `bundle/reader.rs`, `bundle/writer.rs`, `ndjson.rs`, `ingest_security_test.rs`.

All use `crate::json_strict::` or `assay_evidence::json_strict::` — path unchanged.

---

## 4) Tests die gedrag bevriezen (must-have)

**Unit tests:** `json_strict::tests` in `mod.rs` (~40 tests)

| Category | Key tests |
|----------|-----------|
| Duplicate keys | `test_rejects_top_level_duplicate`, `test_rejects_unicode_escape_duplicate`, `test_rejects_surrogate_pair_duplicate` |
| Lone surrogates | `test_rejects_lone_high_surrogate`, `test_rejects_lone_low_surrogate`, `test_accepts_valid_surrogate_pair` |
| DoS limits | `test_dos_nesting_depth_limit`, `test_keys_over_limit_rejected`, `test_string_length_over_limit_rejected` |
| Accept | `test_accepts_valid_json`, `test_accepts_same_key_different_objects` |

**Integration:** `tests/ingest_security_test.rs` uses `validate_json_strict` for hostile input.

**Verification:**
```bash
cargo test -p assay-evidence 2>&1 | tail -5
# Expect: test result: ok. 191 passed
```

---

## 5) Grep-gates (checklist)

```bash
# Required
cargo test -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings

# Forbidden-knowledge gates
rg "0xD800|surrogate|from_u32" crates/assay-evidence/src/json_strict/mod.rs     # expect 0
rg "HashSet::|\.insert\(" crates/assay-evidence/src/json_strict/mod.rs          # expect 0
rg "DuplicateKey|ObjectKeyTracker" crates/assay-evidence/src/json_strict/scan.rs # expect 0
rg "next_char|CharIndices" crates/assay-evidence/src/json_strict/dupkeys.rs     # expect 0
```

---

## Minimale 3-stukken samenvatting

1. **json_strict/mod.rs** – public API (`from_str_strict`, `validate_json_strict`), JsonValidator, value parsing
2. **json_strict/scan.rs** – `parse_json_string()` met unicode/surrogate decoding
3. **json_strict/dupkeys.rs** – ObjectKeyTracker voor duplicate key + TooManyKeys

**API paths unchanged.** Re-export in `mod.rs`; `use crate::json_strict::validate_json_strict` werkt ongewijzigd.
