# Split Review Pack — Wave 1 Step 1

PR: https://github.com/Rul1an/assay/pull/330
Branch: `codex/wave1-step1-behavior-freeze`
Commit: `20889c3244248c81e8f8595f9cdba59dff25a22f`

## Scope

Step 1 is behavior-freeze only.

- Included:
  - inventory snapshot
  - checklist policy knobs
  - behavior-freeze tests
  - boundary grep-gate definitions
- Excluded intentionally:
  - perf/alloc/no-regret optimizations
  - mechanical module splits

## Inventory Snapshot (pre-split)

```bash
git rev-parse HEAD
# b702baefa7547a7ca6ad9ae5d4becc61ff38971c

wc -l crates/assay-registry/src/verify.rs
# 1065

wc -l crates/assay-evidence/src/bundle/writer.rs
# 1442

rg -n "pub struct BundleWriter|verify_bundle" crates/assay-evidence/src/bundle -S
# writer.rs:122 (BundleWriter)
# writer.rs:384 (verify_bundle)
# writer.rs:693 (verify_bundle_with_limits)
```

## Files Changed in Step 1

- `crates/assay-registry/src/verify.rs`
- `crates/assay-evidence/src/bundle/writer.rs`
- `docs/contributing/SPLIT-CHECKLIST-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-bundle-writer.md`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

## Contract Tests Added

Registry verify contract coverage:

- fail-closed matrix for `verify_pack`:
  - unsigned strict -> `RegistryError::Unsigned` (+ exit code `4`)
  - malformed signature -> `RegistryError::SignatureInvalid` (+ exit code `4`)
  - digest mismatch dominates permissive options -> `RegistryError::DigestMismatch` (+ exit code `4`)
  - `allow_unsigned=true` -> success (`signed=false`)
  - `skip_signature=true` -> success path without signature verification
- malformed signature reason bucket determinism on identical input

Writer contract coverage:

- byte determinism positive: same normalized events -> identical bytes
- byte determinism negative: changed payload -> bytes differ
- stable typed code coverage:
  - unexpected file -> `ContractUnexpectedFile`
  - path traversal -> `SecurityPathTraversal`
  - event-count limit -> `LimitTotalEvents`
  - file-size limit -> `LimitFileSize`

## How To Verify Locally

```bash
cargo test -p assay-registry test_verify_pack_fail_closed_matrix_contract
cargo test -p assay-registry test_verify_pack_malformed_signature_reason_is_stable

cargo test -p assay-evidence test_verify_limits_enforced
cargo test -p assay-evidence test_bundle_bytes_are_deterministic_for_same_input
cargo test -p assay-evidence test_bundle_bytes_change_when_event_payload_changes
cargo test -p assay-evidence test_verify_unexpected_file_has_stable_contract_code
cargo test -p assay-evidence test_verify_path_traversal_has_stable_security_code
```

## Boundary Policy Knobs

See:

- `docs/contributing/SPLIT-CHECKLIST-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-bundle-writer.md`

Key points:

- `wire.rs` is parsing-only.
- `policy.rs` no IO/network, crypto-agnostic.
- `dsse.rs` policy-agnostic.
- writer split keeps `tar_io.rs` deterministic-only and `limits.rs` as single source of truth.

## Known Limitations

- Perf/no-regret changes are intentionally excluded from Step 1.
- Wave 1 Step 2 and Step 3 will do mechanical splits first; optimizations remain a separate follow-up step.
