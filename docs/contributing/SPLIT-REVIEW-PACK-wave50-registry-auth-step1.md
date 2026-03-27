# Wave50 Registry Auth Step1 Review Pack

## Intent

Freeze `crates/assay-registry/src/auth.rs` before any split, while pinning both the direct auth
semantics and the downstream registry-client auth-header contract.

## Scope

- `docs/contributing/SPLIT-PLAN-wave50-registry-auth.md`
- `docs/contributing/SPLIT-CHECKLIST-wave50-registry-auth-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave50-registry-auth-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave50-registry-auth-step1.md`
- `scripts/ci/review-wave50-registry-auth-step1.sh`

## Non-goals

- no workflow changes
- no edits under `crates/assay-registry/src/**`
- no edits under `crates/assay-registry/tests/**`
- no resolver / trust / client behavior changes
- no CLI or evidence coupling changes
- no auth semantic cleanup yet

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-wave50-registry-auth-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
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

1. Confirm the diff is limited to the five Step1 files.
2. Confirm the plan freezes `TokenProvider` / `OidcProvider` semantics rather than proposing redesign.
3. Confirm the move-map names the exact `auth_next/*` seams and the downstream client-header contract.
4. Confirm the reviewer script blocks any registry source or registry test edits.
5. Confirm both lib auth tests and downstream registry-client auth tests are pinned.
