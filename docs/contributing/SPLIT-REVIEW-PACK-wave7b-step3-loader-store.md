# Wave7B Step3 review pack: loader/store closure

Intent:
- Finalize closure after Step2 split:
  - make `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader.rs` testless and delegation-only.
  - move loader unit tests to `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader_internal/tests.rs`.
  - keep store Step2 boundaries enforced.

Executed validation:
```bash
cargo fmt --check
cargo clippy -p assay-evidence -p assay-core --all-targets -- -D warnings
cargo check -p assay-evidence -p assay-core
BASE_REF=origin/main bash scripts/ci/review-wave7b-step3.sh
```

Key proof snippets:
```rust
// loader facade
pub fn load_pack(reference: &str) -> Result<LoadedPack, PackError> {
    loader_internal::run::load_pack_impl(reference)
}
```

```bash
rg -n '^\s*#\[cfg\(test\)\]|^\s*mod\s+tests\s*[{;]|^\s*fn\s+' crates/assay-evidence/src/lint/packs/loader.rs
# expected: no matches
```

Anchor execution fix:
- Step3 reviewer script runs loader anchors by fully-qualified test names:
  - `lint::packs::loader::loader_internal::tests::test_*`
- This avoids false green from `--exact` with incomplete test names.

LOC snapshot:
- `/Users/roelschuurkes/assay/crates/assay-evidence/src/lint/packs/loader.rs`: `793 -> 106`
- `/Users/roelschuurkes/assay/crates/assay-core/src/storage/store.rs`: `774 -> 658` (unchanged in Step3)

Risk:
- Low: test relocation + facade-thinness closure only; no public API changes.
