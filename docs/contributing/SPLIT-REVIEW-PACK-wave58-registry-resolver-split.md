# Wave58 Registry Resolver Review Pack

PR type:
- `refactor(registry): split resolver facade`

Reviewer focus:
- Confirm `resolver.rs` is only a facade and public re-export surface.
- Confirm moved route logic is mechanically equivalent to the prior `resolver.rs`.
- Confirm `resolver_next` did not widen private resolver fields or expose new crate APIs.
- Confirm Wave57 resolver contracts still pass.
- Confirm no `304 Not Modified` behavior is changed or claimed.

Expected changed paths:
- `crates/assay-registry/src/resolver.rs`
- `crates/assay-registry/src/resolver_next/mod.rs`
- `crates/assay-registry/src/resolver_next/local.rs`
- `crates/assay-registry/src/resolver_next/bundled.rs`
- `crates/assay-registry/src/resolver_next/registry.rs`
- `crates/assay-registry/src/resolver_next/byos.rs`
- `crates/assay-registry/src/resolver_next/tests.rs`
- `docs/contributing/SPLIT-PLAN-wave58-registry-resolver-split.md`
- `docs/contributing/SPLIT-CHECKLIST-wave58-registry-resolver-split.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave58-registry-resolver-split.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave58-registry-resolver-split.md`
- `scripts/ci/review-wave58-registry-resolver-split.sh`

Verification commands:
- `cargo fmt --check`
- `cargo check -p assay-registry`
- `cargo test -p assay-registry resolver`
- `cargo test -p assay-registry --test resolver_contracts`
- `cargo clippy -p assay-registry --all-targets -- -D warnings`
- `BASE_REF=origin/main bash scripts/ci/review-wave58-registry-resolver-split.sh`

Merge posture:
- Merge only after local gate, GitHub checks, and review-comment sweep are clean.
