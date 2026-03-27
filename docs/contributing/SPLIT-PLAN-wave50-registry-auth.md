# Wave50 Plan - `registry/auth.rs` Kernel Split

## Goal

Split `crates/assay-registry/src/auth.rs` behind a stable facade with zero registry-auth semantic
drift and no downstream client, resolver, or trust coupling drift.

Current hotspot baseline on `origin/main @ 91bdd8b2`:
- `crates/assay-registry/src/auth.rs`: `685` LOC
- `crates/assay-registry/src/client/http.rs`: auth-header and unauthorized-response companion
- `crates/assay-registry/tests/registry_client/scenarios_auth_headers.rs`: downstream auth-header contract companion
- `crates/assay-core/src/mcp/proxy.rs`: next `P1` runtime companion after `R50`

## Step1 (freeze)

Branch: `codex/wave50-registry-auth-step1` (base: `main`)

Deliverables:
- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step1.md`
- `scripts/ci/review-wave50-registry-auth-step1.sh`

Step1 constraints:
- docs+gate only
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- no workflow edits
- no client / resolver / trust / CLI / evidence edits

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
  - `cargo test -q -p assay-registry --lib 'auth::tests::test_static_token' -- --exact`
  - `cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_static' -- --exact`
  - `cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_empty_token' -- --exact`
  - `cargo test -q -p assay-registry --lib 'auth::tests::test_get_static_token' -- --exact`
  - `cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_full_flow' -- --exact`
  - `cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_github_failure' -- --exact`
  - `cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_cache_clear' -- --exact`
  - `cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_token_expiry_triggers_refresh' -- --exact`
  - `cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_retry_backoff_on_failure' -- --exact`
  - `cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_authentication_header' -- --exact`
  - `cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_no_auth_when_no_token' -- --exact`
  - `cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_pack_fetch::test_fetch_pack_unauthorized' -- --exact`

## Frozen public surface

Wave50 freezes the expectation that Step2 keeps these auth entrypoints and consumer-facing
contracts unchanged in meaning:
- `TokenProvider`
- `TokenProvider::static_token`
- `TokenProvider::from_env`
- `TokenProvider::get_token`
- `TokenProvider::is_authenticated`
- `TokenProvider::github_oidc`
- `OidcProvider`
- `OidcProvider::from_github_actions`
- `OidcProvider::new`
- `OidcProvider::get_token`
- `OidcProvider::clear_cache`

Step2 may reorganize internal ownership behind `auth.rs`, but must not redefine:
- static-token semantics
- environment precedence and empty-token fallback behavior
- OIDC environment detection semantics
- GitHub OIDC request and registry exchange behavior
- cache hit/expiry/refresh behavior
- retry/backoff behavior
- downstream auth-header or unauthorized-response behavior in the registry client

## Status

- T-R1 closed on `main` via `#982`.
- T-R2 closed on `main` via `#985`.
- Wave50 Step1 shipped on `main` via `#986`.
- Wave50 Step2 shipped on `main` via `#987`.
- Wave50 Step3 is the closure/docs+gates-only slice for the shipped auth split.

## Step2 (mechanical split preview)

Branch: `codex/wave50-registry-auth-step2` (base: `main`)

Target layout:
- `crates/assay-registry/src/auth.rs` (thin facade + stable routing)
- `crates/assay-registry/src/auth_next/mod.rs`
- `crates/assay-registry/src/auth_next/providers.rs`
- `crates/assay-registry/src/auth_next/oidc.rs`
- `crates/assay-registry/src/auth_next/cache.rs`
- `crates/assay-registry/src/auth_next/headers.rs`
- `crates/assay-registry/src/auth_next/diagnostics.rs`

Step2 scope:
- `crates/assay-registry/src/auth.rs`
- `crates/assay-registry/src/auth_next/mod.rs`
- `crates/assay-registry/src/auth_next/providers.rs`
- `crates/assay-registry/src/auth_next/oidc.rs`
- `crates/assay-registry/src/auth_next/cache.rs`
- `crates/assay-registry/src/auth_next/headers.rs`
- `crates/assay-registry/src/auth_next/diagnostics.rs`
- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step2.md`
- `scripts/ci/review-wave50-registry-auth-step2.sh`

Step2 principles:
- 1:1 body moves
- stable `TokenProvider` / `OidcProvider` facade behavior
- no static/env precedence drift
- no OIDC exchange or request-header drift
- no cache/refresh drift
- no retry/backoff drift
- no downstream auth-header or unauthorized-response drift
- no downstream header or unauthorized-response drift
- no edits under `crates/assay-registry/tests/**`
- no workflow edits

Current Step2 shape:
- `auth.rs`: stable facade, public `TokenProvider` / `OidcProvider`, private cache structs, and existing inline tests
- `auth_next/providers.rs`: `TokenProvider` constructors, env precedence, and auth-state helpers
- `auth_next/oidc.rs`: GitHub Actions environment detection, OIDC exchange, retry, and registry-token fetch flow
- `auth_next/cache.rs`: cache hit / refresh / clear helpers
- `auth_next/headers.rs`: GitHub OIDC header and request-url helpers
- `auth_next/diagnostics.rs`: network / parse / unauthorized error shaping for OIDC exchange

Current Step2 LOC snapshot on this branch:
- `crates/assay-registry/src/auth.rs`: `685 -> 492`
- `crates/assay-registry/src/auth_next/providers.rs`: `47`
- `crates/assay-registry/src/auth_next/oidc.rs`: `165`
- `crates/assay-registry/src/auth_next/cache.rs`: `32`
- `crates/assay-registry/src/auth_next/headers.rs`: `17`
- `crates/assay-registry/src/auth_next/diagnostics.rs`: `49`

## Step3 (closure)

Branch: `codex/wave50-registry-auth-step3` (base: `main`)

Step3 closes the shipped Wave50 auth split with docs/gates only after Step2 lands on `main`.

Step3 constraints:
- docs+gate only
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- keep `auth.rs` as the stable facade entrypoint
- no new module cuts
- no behavior cleanup beyond internal follow-up notes
- no auth-header, cache, retry, or exchange-path drift

Step3 deliverables:
- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step3.md`
- `scripts/ci/review-wave50-registry-auth-step3.sh`

## Promote

Stacked chain:
- Step1 -> `main`
- Step2 -> Step1
- Step3 -> Step2

Final promote PR to `main` from Step3 once the chain is clean.

## Reviewer notes

This wave must remain registry-auth split planning only.

Primary failure modes:
- sneaking auth behavior cleanup into a mechanical split
- changing env precedence or empty-token behavior while chasing file size
- changing OIDC request/exchange or retry semantics under a refactor label
- drifting client auth-header behavior via helper moves
