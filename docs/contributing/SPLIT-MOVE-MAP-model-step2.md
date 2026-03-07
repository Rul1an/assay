# SPLIT-MOVE-MAP - Wave13 Step2 - `assay-core/src/model.rs`

## Goal

Mechanically split `crates/assay-core/src/model.rs` into bounded modules under
`crates/assay-core/src/model/` with zero behavior drift and stable public surface.

## Target layout

- `crates/assay-core/src/model/mod.rs`
  - facade: `pub` surface + `pub use` + module wiring
  - no heavy logic in facade (thin wrappers only when needed)
- `crates/assay-core/src/model/types.rs`
  - data model: structs/enums/type aliases/constants
- `crates/assay-core/src/model/serde.rs`
  - serde glue: custom serialize/deserialize, visitors, serde mappers
- `crates/assay-core/src/model/validation.rs`
  - pure validation/invariant helpers and pure transforms
- `crates/assay-core/src/model/tests/mod.rs`
  - moved unit tests from `model.rs` (if present)

## Hard boundary rule

- no hidden IO in helpers
- `validation.rs` and `serde.rs` must not read files or env
- if IO is required, use explicit module naming (`io.rs`) and document it first
- default for Wave13: no `io.rs`

## Move inventory rules

### 1) Public surface from `model.rs`

| Source pattern | Target |
| --- | --- |
| `pub struct ...` | `types.rs` (definition), re-export via `mod.rs` |
| `pub enum ...` | `types.rs` |
| `pub type ...` | `types.rs` |
| `pub const ...` | `types.rs` |
| `pub fn ...` | `validation.rs` if pure, otherwise thin facade wrapper only |
| `impl ...` | near owning type in `types.rs`, serde-specific impls in `serde.rs` |

Facade policy for `mod.rs`:
- only `mod types; mod serde; mod validation; #[cfg(test)] mod tests;`
- central `pub use types::{...};`
- no large helper function bodies

### 2) Serde glue to `serde.rs`

| Source marker | Target |
| --- | --- |
| custom `deserialize_*` / `serialize_*` functions | `serde.rs` |
| `Visitor` structs and impls | `serde.rs` |
| string-to-enum mapping helpers tied to serde | `serde.rs` |
| `#[derive(Serialize, Deserialize)]` on type | type remains in `types.rs` |

Constraint: `serde.rs` can use serde crates, but no filesystem IO.

### 3) Validation and invariants to `validation.rs`

| Source marker | Target |
| --- | --- |
| `validate_*` functions | `validation.rs` |
| `normalize_*` functions | `validation.rs` |
| pure shape detection/check helpers | `validation.rs` |
| pointer/string transforms (pure) | `validation.rs` |

Constraint: no `std::fs`, `PathBuf`, `read_to_string`, `env::` in `validation.rs`.

### 4) Tests relocation

- move `#[cfg(test)]` unit tests from `model.rs` to `model/tests/mod.rs`
- keep test names and assertions unchanged unless explicitly allowed in plan
- keep `#[cfg(test)] mod tests;` in facade

## Inventory commands (Step2 prep)

```bash
# public items
rg -n '^(pub (struct|enum|type|const|fn)|impl )' crates/assay-core/src/model.rs

# serde markers
rg -n '#\\[serde|Serialize|Deserialize|Visitor|deserialize_|serialize_' crates/assay-core/src/model.rs

# validation markers
rg -n 'validate_|normalize_|invariant|ensure_|check_' crates/assay-core/src/model.rs

# IO markers (must not move into validation.rs/serde.rs)
rg -n 'std::fs|read_to_string|File|PathBuf|OpenOptions|create_dir|env::' crates/assay-core/src/model.rs
```

## Mechanical move workflow

1. move blocks 1:1 to target module
2. only fix `use` paths and visibility as needed
3. keep `mod.rs` as wiring/re-export center
4. run `cargo fmt` after each move batch, avoid cleanup edits during mechanical pass

## Reviewer parity checks

### API parity
- `assay_core::model::*` exports remain stable
- no semver-visible rename/remove/path drift

### Behavior parity
- logic unchanged, only relocation
- serde behavior stable (existing tests remain green)

### Boundary parity
- no IO in `validation.rs`
- no IO in `serde.rs`
- no heavy logic in `mod.rs`

## Step2 allowlist preview

- `crates/assay-core/src/model.rs` (facade transition)
- `crates/assay-core/src/model/**`
- `docs/contributing/SPLIT-CHECKLIST-model-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-model-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-model-step2.md`
- `scripts/ci/review-model-step2.sh`
