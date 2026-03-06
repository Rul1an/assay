# Registry Client Step2 Move Map (Mechanical Split)

## Scope

- Source: `crates/assay-registry/tests/registry_client.rs`
- Destination root: `crates/assay-registry/tests/registry_client/`
- Facade: `crates/assay-registry/tests/registry_client.rs`

## Moves

| Old location | New location | Notes |
| --- | --- | --- |
| Top-level module docs/imports | `registry_client/mod.rs` | Shared imports + module wiring only |
| `create_test_client` helper | `registry_client/support.rs` | Signature unchanged |
| Pack fetch + status tests | `registry_client/scenarios_pack_fetch.rs` | Pure move |
| Version/meta/keys tests | `registry_client/scenarios_meta_keys.rs` | Pure move |
| Auth/user-agent/vary tests | `registry_client/scenarios_auth_headers.rs` | Pure move |
| Signature sidecar tests | `registry_client/scenarios_signature.rs` | Pure move |
| ETag/digest/cache tests | `registry_client/scenarios_cache_digest.rs` | Pure move |
| Retry-specific tests | `registry_client/scenarios_retry.rs` | Pure move |

## Test Inventory Mapping

| Scenario file | Test functions |
| --- | --- |
| `scenarios_pack_fetch.rs` | `test_fetch_pack_success`, `test_fetch_pack_304_not_modified`, `test_fetch_pack_not_found`, `test_fetch_pack_unauthorized`, `test_fetch_pack_revoked_header_only`, `test_fetch_pack_revoked_with_body`, `test_rate_limiting_with_retry_after` |
| `scenarios_meta_keys.rs` | `test_list_versions`, `test_get_pack_meta`, `test_fetch_keys_manifest` |
| `scenarios_auth_headers.rs` | `test_authentication_header`, `test_no_auth_when_no_token`, `test_user_agent_header`, `test_vary_header_for_authenticated_response` |
| `scenarios_signature.rs` | `test_fetch_signature_sidecar`, `test_fetch_signature_sidecar_not_found`, `test_fetch_pack_with_signature_signature_500_error_bubbled`, `test_fetch_pack_with_signature_invalid_json_error_bubbled`, `test_fetch_pack_with_signature`, `test_commercial_pack_signature_required_via_sidecar_only` |
| `scenarios_cache_digest.rs` | `test_pack_304_signature_still_valid`, `test_etag_is_strong_etag_format`, `test_content_digest_vs_canonical_digest`, `test_304_cache_hit_flow` |
| `scenarios_retry.rs` | `test_retry_on_429_with_retry_after`, `test_max_retries_exceeded` |

## Facade Wiring

`crates/assay-registry/tests/registry_client.rs` contains only:

```rust
#[path = "registry_client/mod.rs"]
mod registry_client;
```

No public API symbols change in Step2.
