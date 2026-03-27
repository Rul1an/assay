# Wave50 Registry Auth Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step3.md`
- `scripts/ci/review-wave50-registry-auth-step3.sh`
- no code changes in Step3
- no workflow changes

## Closure invariants

- re-run Step2 quality checks (`fmt`, `clippy`, `cargo check`)
- re-run Step2 auth invariants (`TokenProvider` / `OidcProvider` facade still stable in `auth.rs`)
- re-run Step2 behavior pins (static/env precedence, OIDC exchange flow, cache/refresh, retry/backoff)
- re-run downstream registry-client auth-header and unauthorized-response selectors
- keep `auth_next/*` as the only internal implementation split

## Gate requirements

- allowlist-only diff vs `origin/main`
- workflow-ban (`.github/workflows/*`)
- hard fail on tracked changes in `crates/assay-registry/src/**`
- hard fail on untracked files in `crates/assay-registry/src/**`
- hard fail on tracked changes in `crates/assay-registry/tests/**`
- hard fail on untracked files in `crates/assay-registry/tests/**`
- quality checks:
  - `cargo fmt --all --check`
  - `cargo clippy -q -p assay-registry --all-targets -- -D warnings`
  - `cargo check -q -p assay-registry`
  - `cargo check -q -p assay-registry --features oidc`
- pinned tests:
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

## Definition of done

- `BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step3.sh` passes
- Step3 diff is docs+script only
- `auth.rs` remains the stable entrypoint and `auth_next/*` remains the internal implementation seam
