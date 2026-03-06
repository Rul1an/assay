# Registry Client Step2 Checklist (Mechanical Split)

Scope lock:
- mechanical split only
- no test behavior changes
- no retry/timeout tuning
- no fixture content changes
- no workflow changes

## Goal

Split `crates/assay-registry/tests/registry_client.rs` into scoped modules under
`crates/assay-registry/tests/registry_client/` while preserving test names, assertions,
mock wiring, and contract behavior.

## Target files

- `crates/assay-registry/tests/registry_client.rs`
- `crates/assay-registry/tests/registry_client/mod.rs`
- `crates/assay-registry/tests/registry_client/support.rs`
- `crates/assay-registry/tests/registry_client/scenarios_pack_fetch.rs`
- `crates/assay-registry/tests/registry_client/scenarios_meta_keys.rs`
- `crates/assay-registry/tests/registry_client/scenarios_auth_headers.rs`
- `crates/assay-registry/tests/registry_client/scenarios_signature.rs`
- `crates/assay-registry/tests/registry_client/scenarios_cache_digest.rs`
- `crates/assay-registry/tests/registry_client/scenarios_retry.rs`
- `docs/contributing/SPLIT-CHECKLIST-registry-client-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-registry-client-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step2.md`
- `scripts/ci/review-registry-client-step2.sh`

## Hard gates

- `cargo fmt --check`
- `cargo clippy -p assay-registry --tests -- -D warnings`
- `cargo test -p assay-registry --tests`
- facade gate:
  - `registry_client.rs` is thin wiring only (`#[path = "registry_client/mod.rs"]` + `mod registry_client;`)
  - no inline `#[tokio::test]` remains in `registry_client.rs`
- inventory parity gate:
  - count of `test_*` functions equals pre-split count from `BASE_REF`
- diff allowlist only
- workflow-ban (`.github/workflows/*` forbidden)

## Acceptatiecriteria

- all tests compile/run from module tree under `tests/registry_client/`
- all test names remain unchanged for CI history continuity
- wiremock behavior and bind pattern unchanged
- reviewer script passes end-to-end
