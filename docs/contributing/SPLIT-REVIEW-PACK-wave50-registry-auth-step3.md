# Wave50 Registry Auth Step3 Review Pack

## Intent

Close the shipped `registry/auth.rs` split with docs/gates only, while re-running the Step2 auth
invariants and proving no follow-on drift in registry-client auth behavior.

## Scope

- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step3.md`
- `scripts/ci/review-wave50-registry-auth-step3.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- no auth semantic redesign
- no env precedence, OIDC exchange, cache, retry, or downstream header drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step3.sh
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

1. Confirm the diff is limited to the Step3 docs and the reviewer script.
2. Confirm the plan records that Step2 shipped on `main` via `#987`.
3. Confirm Step3 does not touch `crates/assay-registry/src/**` or `crates/assay-registry/tests/**`.
4. Confirm the reviewer script re-runs both auth-lib and downstream registry-client selectors.
5. Confirm the closure docs say there are no new module cuts in Step3.
