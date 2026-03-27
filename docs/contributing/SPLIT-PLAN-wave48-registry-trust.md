# Wave48 Plan — `registry/trust.rs` Kernel Split

## Goal

Split `crates/assay-registry/src/trust.rs` behind a stable facade with zero trust-store semantic
drift and no downstream resolver or verification contract drift.

Current hotspot baseline on `origin/main @ ba8ef734`:
- `crates/assay-registry/src/trust.rs`: `838` LOC
- `crates/assay-registry/src/auth.rs`: `685` LOC
- `crates/assay-registry/tests/resolver_production_roots.rs`: production-root resolver companion
- `crates/assay-registry/src/verify_internal/tests/dsse.rs`: trust lookup companion

## Step1 (freeze)

Branch: `codex/wave48-registry-trust-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
- `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step1.md`
- `scripts/ci/review-wave48-registry-trust-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- no workflow edits
- no verify / resolver / client / CLI / evidence edits

Step1 gate:
- allowlist-only diff (the 5 Step1 files)
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-registry/src/**`
- hard fail on untracked files in `crates/assay-registry/src/**`
- hard fail on tracked changes in `crates/assay-registry/tests/**`
- hard fail on untracked files in `crates/assay-registry/tests/**`
- `cargo fmt --check`
- `cargo clippy -p assay-registry --all-targets -- -D warnings`
- targeted tests:
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_with_production_roots_loads_embedded_roots' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_add_from_manifest' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_pinned_key_not_overwritten' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_needs_refresh' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_revoke_old_key' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_pinned_root_survives_revocation' -- --exact`
  - `cargo test -q -p assay-registry --lib 'trust::tests::test_trust_rotation_key_expires_after_added' -- --exact`
  - `cargo test -q -p assay-registry --test resolver_production_roots resolver_accepts_signed_pack_with_embedded_production_root -- --exact`

## Frozen public surface

Wave48 freezes the expectation that Step2 keeps these trust-store entrypoints and consumer-facing
contracts unchanged in meaning:
- `TrustStore`
- `KeyMetadata`
- `TrustStore::new`
- `TrustStore::from_pinned_roots`
- `TrustStore::from_production_roots`
- `TrustStore::add_pinned_key`
- `TrustStore::add_from_manifest`
- `TrustStore::get_key_async`
- `TrustStore::get_key`
- `TrustStore::needs_refresh`
- `TrustStore::is_trusted`
- `TrustStore::list_keys`
- `TrustStore::get_metadata`
- `TrustStore::clear_cached_keys`

Step2 may reorganize internal ownership behind `trust.rs`, but must not redefine:
- pinned-root loading semantics
- key-id verification semantics
- revoked/expired-key handling
- manifest refresh / cache TTL behavior
- sync/async key lookup behavior
- production-root resolver behavior
- downstream verification trust lookup semantics

## Status

- Wave47 closed on `main` via `#968`.
- Wave48 Step1 shipped on `main` via `#969`.
- Wave48 Step2 shipped on `main` via `#970`.
- Step3 is the closure slice for the shipped `trust.rs` split.

## Step2 (mechanical split preview)

Branch: `codex/wave48-registry-trust-step2` (base: `main`)

Target layout:
- `crates/assay-registry/src/trust.rs` (thin facade + stable routing)
- `crates/assay-registry/src/trust_next/mod.rs`
- `crates/assay-registry/src/trust_next/decode.rs`
- `crates/assay-registry/src/trust_next/pinned.rs`
- `crates/assay-registry/src/trust_next/manifest.rs`
- `crates/assay-registry/src/trust_next/cache.rs`
- `crates/assay-registry/src/trust_next/access.rs`

Step2 scope:
- `crates/assay-registry/src/trust.rs`
- `crates/assay-registry/src/trust_next/mod.rs`
- `crates/assay-registry/src/trust_next/decode.rs`
- `crates/assay-registry/src/trust_next/pinned.rs`
- `crates/assay-registry/src/trust_next/manifest.rs`
- `crates/assay-registry/src/trust_next/cache.rs`
- `crates/assay-registry/src/trust_next/access.rs`
- `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
- `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step2.md`
- `scripts/ci/review-wave48-registry-trust-step2.sh`

Step2 principles:
- 1:1 body moves
- stable `TrustStore` facade behavior
- no pinned-root or key-id verification drift
- no revoked/expired-key drift
- no cache/refresh drift
- no resolver/verify coupling drift
- no edits under `crates/assay-registry/tests/**`
- no workflow edits

Current Step2 shape:
- `trust.rs`: stable facade, public `TrustStore`/`KeyMetadata`, and existing inline tests
- `trust_next/decode.rs`: base64 / SPKI / key-id decode helpers
- `trust_next/pinned.rs`: production-root parsing and pinned insertion helpers
- `trust_next/manifest.rs`: manifest ingest and trust-rotation helpers
- `trust_next/cache.rs`: refresh / cache state helpers
- `trust_next/access.rs`: get/list/metadata access helpers

Current Step2 LOC snapshot on this branch:
- `crates/assay-registry/src/trust.rs`: `838 -> 595`
- `crates/assay-registry/src/trust_next/pinned.rs`: `98`
- `crates/assay-registry/src/trust_next/manifest.rs`: `77`
- `crates/assay-registry/src/trust_next/access.rs`: `43`
- `crates/assay-registry/src/trust_next/cache.rs`: `31`
- `crates/assay-registry/src/trust_next/decode.rs`: `24`

## Step3 (closure)

Step3 will close the shipped Wave48 trust split with docs/gates only once Step2 lands on `main`.

Step3 constraints:
- docs+gate only
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- keep `trust.rs` as the stable facade entrypoint
- no new module cuts
- no behavior cleanup beyond internal follow-up notes
- no pinned-root, manifest, cache, or verification coupling drift

Step3 deliverables:
- `docs/contributing/SPLIT-PLAN-wave48-registry-trust.md`
- `docs/contributing/SPLIT-CHECKLIST-wave48-registry-trust-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave48-registry-trust-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave48-registry-trust-step3.md`
- `scripts/ci/review-wave48-registry-trust-step3.sh`

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Reviewer notes

This wave must remain trust-store split planning only.

Primary failure modes:
- sneaking trust-store behavior cleanup into a mechanical split
- changing pinned-root or manifest semantics while chasing file size
- changing key-id or SPKI validation behavior under a refactor label
- drifting resolver or verification coupling via helper moves
