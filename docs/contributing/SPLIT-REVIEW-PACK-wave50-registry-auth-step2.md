# Wave50 Registry Auth Step2 Review Pack

## Intent

Mechanically split `crates/assay-registry/src/auth.rs` behind a stable facade, while preserving
static/env precedence, OIDC exchange + cache + retry behavior, and downstream registry-client
auth-header contracts.

## Scope

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

## Non-goals

- no workflow changes
- no edits under `crates/assay-registry/tests/**`
- no edits outside the auth seam inside `crates/assay-registry/src/**`
- no auth semantic redesign
- no request-header or unauthorized-response behavior changes
- no cache or retry policy changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step2.sh
```

Gate includes:

```bash
cargo fmt --all --check
cargo clippy -q -p assay-registry --all-targets -- -D warnings
cargo check -q -p assay-registry
cargo check -q -p assay-registry --features oidc
cargo test -q -p assay-registry --lib 'auth::tests::test_static_token' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_static' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_from_env_empty_token' -- --exact
cargo test -q -p assay-registry --lib 'auth::tests::test_get_static_token' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_full_flow' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_github_failure' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_cache_clear' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_token_expiry_triggers_refresh' -- --exact
cargo test -q -p assay-registry --lib --features oidc 'auth::oidc_tests::test_oidc_retry_backoff_on_failure' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_authentication_header' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_auth_headers::test_no_auth_when_no_token' -- --exact
cargo test -q -p assay-registry --test registry_client 'registry_client::scenarios_pack_fetch::test_fetch_pack_unauthorized' -- --exact
```

## Reviewer 60s scan

1. Confirm the diff is limited to `auth.rs`, `auth_next/*`, Step2 docs, and the reviewer script.
2. Confirm `auth.rs` still owns the public types and inline tests, but not the OIDC body logic.
3. Confirm provider/env logic, OIDC exchange logic, cache logic, and error-shaping each moved to the intended module.
4. Confirm no registry-client or registry integration tests were edited.
5. Confirm the reviewer script reruns both auth-lib and downstream auth-header selectors.
