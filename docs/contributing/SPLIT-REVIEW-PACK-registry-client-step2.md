# Registry Client Step2 Review Pack (Mechanical Split)

## Intent

Mechanically split `registry_client` integration tests into scenario modules for
faster review and failure localization, with zero behavior change.

## Scope

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

## Non-goals

- no production code changes
- no retry/backoff tuning
- no fixture or assertion changes
- no workflow changes

## Validation command

```bash
BASE_REF=<step1-branch-or-main> bash scripts/ci/review-registry-client-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-registry --tests -- -D warnings
cargo test -p assay-registry --tests
```

## Reviewer 60s scan

1. Confirm `registry_client.rs` is only a thin module facade.
2. Confirm all tests moved under `tests/registry_client/scenarios_*.rs`.
3. Confirm test-name inventory count is unchanged versus `BASE_REF`.
4. Confirm no files outside allowlist changed and no workflows touched.
5. Run reviewer script and confirm PASS.

## Wiremock note

Wiremock tests require local port binding; CI handles this. Local runs may need an unsandboxed environment.
