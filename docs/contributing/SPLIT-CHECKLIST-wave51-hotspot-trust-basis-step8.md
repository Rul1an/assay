# SPLIT CHECKLIST - Wave 51 Trust Basis Step8

## Scope Lock

- Freeze Trust Basis protocol behavior before splitting `crates/assay-evidence/src/trust_basis.rs`.
- Add contract tests for generated claim order, canonical JSON shape, and diff report ordering/summary.
- Do not move implementation into modules in Step 8.
- Do not change public re-exports in `crates/assay-evidence/src/lib.rs`.
- Do not change CLI command behavior, JSON schema strings, claim IDs, claim levels, sources, boundaries, workflows, or generated files.

## Files

- `crates/assay-evidence/src/trust_basis.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-trust-basis-step8.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-trust-basis-step8.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-trust-basis-step8.md`
- `scripts/ci/review-wave51-hotspot-trust-basis-step8.sh`

## Drift Gates

- `trust_basis.rs` still owns `TrustBasis`, `TrustBasisClaim`, claim enums, diff structs, generation, canonical serialization, classifier helpers, and tests.
- No `trust_basis/` implementation directory is introduced in Step 8.
- `generate_trust_basis`, `to_canonical_json_bytes`, `diff_trust_basis`, and `duplicate_trust_basis_claim_ids` remain in `trust_basis.rs`.
- Contract tests with prefix `trust_basis_contract_` cover:
  - generated claim-id order
  - canonical JSON field/claim shape
  - diff report level order, section ordering, and summary counters
- Workflow files and generated eBPF bindings remain untouched.

## Validation

```bash
cargo fmt --check
cargo check -p assay-evidence
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib trust_basis_contract_
cargo test -p assay-evidence --lib trust_basis
bash scripts/ci/review-wave51-hotspot-trust-basis-step8.sh
```

## Definition of Done

- Step 8 reviewer script passes.
- Trust Basis contract tests pass.
- No module split occurs before the frozen behavior contracts exist.
- Step 9 can mechanically split types/diff/generation/classifiers behind the same public `trust_basis` API.
