# Review Pack: ADR-024 Epic 1 (VerifyLimitsOverrides)

**Branch:** `feat/adr-024-sim-hardening`
**Epic:** E1 — `VerifyLimitsOverrides` in assay-evidence; `apply()` merge; `deny_unknown_fields`
**ADR:** [ADR-024 Sim Engine Hardening](./ADR-024-Sim-Engine-Hardening.md)

---

## Review Checklist

### Functional

| Criterion | Location | Verify |
|-----------|----------|--------|
| `VerifyLimitsOverrides` exists with all 8 fields as `Option<T>` | `writer.rs` L544–553 | Fields match `VerifyLimits` 1:1 |
| `#[serde(deny_unknown_fields)]` present | `writer.rs` L543 | Unknown keys fail deserialize |
| `VerifyLimits::apply(overrides)` performs partial merge | `writer.rs` L556–571 | Only `Some` overrides; others from `self` |
| Re-exported from `assay_evidence` | `lib.rs` | `use assay_evidence::VerifyLimitsOverrides` works |

### Tests

| Test | Purpose |
|------|---------|
| `test_verify_limits_overrides_merge` | Partial JSON → only provided fields override; defaults preserved |
| `test_verify_limits_overrides_deny_unknown_fields` | `{"max_bundle_bytess": 1}` → deserialize fails |
| `test_verify_limits_overrides_empty_roundtrip` | `{}` → all None → apply yields identity (equals default) |
| `test_verify_limits_overrides_drift_guard` | Destructure both structs → compile fails if field count drifts |

### ADR Alignment

| ADR § | Requirement | Status |
|-------|-------------|--------|
| Limits Model | `VerifyLimitsOverrides` + `deny_unknown_fields` | ✓ |
| Merge | `defaults.apply(overrides)`; only provided keys override | ✓ |
| Location | assay-evidence (co-located with VerifyLimits) | ✓ |

---

## Verification Commands

```bash
# Run Epic 1 unit tests
cargo test -p assay-evidence --lib verify_limits_overrides

# Run full assay-evidence test suite
cargo test -p assay-evidence --lib

# Clippy
cargo clippy -p assay-evidence -- -D warnings
```

---

## Line-by-Line Snippet (paste & sanity check)

```rust
/// Resource limits for bundle verification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VerifyLimits {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: usize,
    pub max_line_bytes: usize,
    pub max_path_len: usize,
    pub max_json_depth: usize,
}

impl Default for VerifyLimits {
    fn default() -> Self {
        Self {
            max_bundle_bytes: 100 * 1024 * 1024,  // 100 MB compressed
            max_decode_bytes: 1024 * 1024 * 1024, // 1 GB uncompressed
            max_manifest_bytes: 10 * 1024 * 1024, // 10 MB
            max_events_bytes: 500 * 1024 * 1024,  // 500 MB
            max_events: 100_000,
            max_line_bytes: 1024 * 1024,          // 1 MB
            max_path_len: 256,
            max_json_depth: 64,
        }
    }
}

/// Partial overrides for `VerifyLimits`. Used for CLI/config JSON parsing.
/// Unknown keys cause deserialization to fail (deny_unknown_fields).
/// Merge with `VerifyLimits::default().apply(overrides)`.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerifyLimitsOverrides {
    pub max_bundle_bytes: Option<u64>,
    pub max_decode_bytes: Option<u64>,
    pub max_manifest_bytes: Option<u64>,
    pub max_events_bytes: Option<u64>,
    pub max_events: Option<usize>,
    pub max_line_bytes: Option<usize>,
    pub max_path_len: Option<usize>,
    pub max_json_depth: Option<usize>,
}

impl VerifyLimits {
    /// Apply overrides onto these defaults. Only `Some` values override.
    pub fn apply(self, overrides: VerifyLimitsOverrides) -> VerifyLimits {
        VerifyLimits {
            max_bundle_bytes: overrides.max_bundle_bytes.unwrap_or(self.max_bundle_bytes),
            max_decode_bytes: overrides.max_decode_bytes.unwrap_or(self.max_decode_bytes),
            max_manifest_bytes: overrides.max_manifest_bytes.unwrap_or(self.max_manifest_bytes),
            max_events_bytes: overrides.max_events_bytes.unwrap_or(self.max_events_bytes),
            max_events: overrides.max_events.unwrap_or(self.max_events),
            max_line_bytes: overrides.max_line_bytes.unwrap_or(self.max_line_bytes),
            max_path_len: overrides.max_path_len.unwrap_or(self.max_path_len),
            max_json_depth: overrides.max_json_depth.unwrap_or(self.max_json_depth),
        }
    }
}
```

**Checklist per line:**
- `VerifyLimits`: `u64` × 4, `usize` × 4; `PartialEq, Eq` for roundtrip test ✓
- `VerifyLimitsOverrides`: `Option<u64>` × 4, `Option<usize>` × 4 ✓
- `Deserialize` on Overrides (not Serialize—CLI input only) ✓
- `#[serde(deny_unknown_fields)]` ✓
- `apply()`: `unwrap_or(self.X)` — defaults win when override is `None` ✓

---

## Manual Smoke

```rust
use assay_evidence::{VerifyLimits, VerifyLimitsOverrides};

// Partial override
let overrides: VerifyLimitsOverrides = serde_json::from_str(r#"{"max_bundle_bytes": 1000}"#)?;
let limits = VerifyLimits::default().apply(overrides);
assert_eq!(limits.max_bundle_bytes, 1000);
assert_eq!(limits.max_decode_bytes, 1024 * 1024 * 1024); // default preserved

// Unknown key → error
let err = serde_json::from_str::<VerifyLimitsOverrides>(r#"{"typo": 1}"#).unwrap_err();
```

---

## Merge Gates

- [ ] `cargo test -p assay-evidence --lib` passes
- [ ] `cargo clippy -p assay-evidence -- -D warnings` passes
- [ ] `VerifyLimitsOverrides` re-export works from assay-sim / assay-cli (compile check)
- [ ] Empty roundtrip + drift-guard tests pass

## Future Pitfalls (Epic 2+)

- `{"max_bundle_bytes": -1}` / `1.5` → serde rejects; CLI should surface error cleanly
- Error message quality: avoid opaque serde errors in CLI

## Acceptance

- [ ] All checklist items pass
- [ ] ADR-024 Epics table present; E1 marked implemented
