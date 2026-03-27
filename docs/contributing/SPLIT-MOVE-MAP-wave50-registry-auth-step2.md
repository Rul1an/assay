# SPLIT-MOVE-MAP - Wave50 Step2 - `registry/auth.rs`

## Goal

Mechanically decompose `crates/assay-registry/src/auth.rs` behind a stable facade without changing
registry-auth behavior, OIDC/cache semantics, or downstream client auth-header contracts.

## New layout

- `crates/assay-registry/src/auth.rs`
- `crates/assay-registry/src/auth_next/mod.rs`
- `crates/assay-registry/src/auth_next/providers.rs`
- `crates/assay-registry/src/auth_next/oidc.rs`
- `crates/assay-registry/src/auth_next/cache.rs`
- `crates/assay-registry/src/auth_next/headers.rs`
- `crates/assay-registry/src/auth_next/diagnostics.rs`

## Mapping applied

- `auth.rs`
  - public `TokenProvider` enum
  - public `OidcProvider` struct
  - wrapper methods that preserve the stable facade
  - existing inline tests
- `auth_next/providers.rs`
  - `TokenProvider::static_token`
  - `TokenProvider::from_env`
  - `TokenProvider::get_token`
  - `TokenProvider::is_authenticated`
  - `TokenProvider::github_oidc`
- `auth_next/oidc.rs`
  - `OidcProvider::from_github_actions`
  - `OidcProvider::new`
  - `OidcProvider::exchange_token_with_retry`
  - `OidcProvider::exchange_token`
  - `OidcProvider::get_github_oidc_token`
  - `OidcProvider::exchange_for_registry_token`
- `auth_next/cache.rs`
  - `OidcProvider::get_token`
  - `OidcProvider::clear_cache`
  - cache storage/write helpers
- `auth_next/headers.rs`
  - OIDC request URL / bearer / accept / content-type helpers
- `auth_next/diagnostics.rs`
  - GitHub OIDC and token-exchange error-shaping helpers

## Frozen behavior boundaries

- identical static-token behavior
- identical environment precedence and empty-token fallback behavior
- identical OIDC enablement and GitHub Actions environment detection
- identical GitHub OIDC request + registry exchange behavior
- identical cache hit / expiry / refresh behavior
- identical retry/backoff behavior
- identical downstream auth-header and unauthorized-response behavior in the registry client
- no visibility widening outside the auth split seam

## Step2 anchor selectors

- `auth::tests::test_static_token`
- `auth::tests::test_from_env_static`
- `auth::tests::test_from_env_empty_token`
- `auth::tests::test_get_static_token`
- `auth::oidc_tests::test_oidc_full_flow`
- `auth::oidc_tests::test_oidc_github_failure`
- `auth::oidc_tests::test_oidc_cache_clear`
- `auth::oidc_tests::test_token_expiry_triggers_refresh`
- `auth::oidc_tests::test_oidc_retry_backoff_on_failure`
- `registry_client::scenarios_auth_headers::test_authentication_header`
- `registry_client::scenarios_auth_headers::test_no_auth_when_no_token`
- `registry_client::scenarios_pack_fetch::test_fetch_pack_unauthorized`
