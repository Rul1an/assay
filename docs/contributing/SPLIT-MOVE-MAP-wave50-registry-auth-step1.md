# SPLIT-MOVE-MAP - Wave50 Step1 - `registry/auth.rs`

## Goal

Freeze the current `crates/assay-registry/src/auth.rs` behavior before any mechanical split work.

## Planned Step2 layout

- `crates/assay-registry/src/auth.rs`
- `crates/assay-registry/src/auth_next/mod.rs`
- `crates/assay-registry/src/auth_next/providers.rs`
- `crates/assay-registry/src/auth_next/oidc.rs`
- `crates/assay-registry/src/auth_next/cache.rs`
- `crates/assay-registry/src/auth_next/headers.rs`
- `crates/assay-registry/src/auth_next/diagnostics.rs`

## Frozen behavior boundaries

- identical static-token behavior
- identical environment precedence and empty-token fallback behavior
- identical OIDC enablement and GitHub Actions environment detection
- identical GitHub OIDC request + registry exchange behavior
- identical cache hit / expiry / refresh behavior
- identical retry/backoff behavior
- identical downstream auth-header and unauthorized-response behavior in the registry client

## Step1 anchor selectors

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
