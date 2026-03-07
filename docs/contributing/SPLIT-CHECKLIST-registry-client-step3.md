# Registry Client Step3 Checklist (Closure)

Scope lock:
- closure/docs/gates only
- no workflow changes
- no behavior changes

## Goal

Close Wave11 `registry_client` split with strict closure guards:
- keep `registry_client.rs` as a thin facade
- keep module wiring explicit in `registry_client/mod.rs`
- keep test inventory parity fixed at 26
- enforce allowlist-only diff scope

## Final layout invariants

- `crates/assay-registry/tests/registry_client.rs` stays facade-only:
  - `#[path = "registry_client/mod.rs"]`
  - `mod registry_client;`
- `crates/assay-registry/tests/registry_client/mod.rs` explicitly declares:
  - `mod support;`
  - `mod scenarios_pack_fetch;`
  - `mod scenarios_meta_keys;`
  - `mod scenarios_auth_headers;`
  - `mod scenarios_signature;`
  - `mod scenarios_cache_digest;`
  - `mod scenarios_retry;`
- `crates/assay-registry/tests/registry_client/support.rs` retains shared `create_test_client` helper
- scenario test inventory remains exactly `26`

## Required checks

- `cargo fmt --check`
- `cargo clippy -p assay-registry --tests -- -D warnings`
- `cargo test -p assay-registry --tests`
- `BASE_REF=origin/main bash scripts/ci/review-registry-client-step3.sh`

## Diff allowlist (Step3)

- `crates/assay-registry/tests/registry_client.rs`
- `crates/assay-registry/tests/registry_client/*.rs`
- `docs/contributing/SPLIT-CHECKLIST-registry-client-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step3.md`
- `scripts/ci/review-registry-client-step3.sh`

## Acceptatiecriteria

- closure script passes end-to-end
- no workflow edits
- no test inventory drift (`26`)
- no hidden logic reintroduced in `registry_client.rs`
