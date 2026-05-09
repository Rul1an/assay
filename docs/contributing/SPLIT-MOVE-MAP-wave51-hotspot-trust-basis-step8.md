# SPLIT MOVE MAP - Wave 51 Trust Basis Step8

## Intent

Step 8 is a freeze-only characterization step for `crates/assay-evidence/src/trust_basis.rs`. The file is the next Wave 51 hotspot after runner, sandbox, and MCP proxy. Before moving code, this step pins the Trust Basis protocol surfaces that reviewers need to protect during future splits.

## Moves

No implementation moves in Step 8.

| Behavior | Frozen By | Notes |
| --- | --- | --- |
| generated claim order | `trust_basis_contract_generated_claim_id_order_is_frozen` | Locks the public claim-id sequence emitted by `generate_trust_basis`. |
| canonical JSON shape | `trust_basis_contract_canonical_json_shape_is_frozen` | Locks pretty JSON field order, enum spellings, null note behavior, and trailing newline. |
| diff report ordering | `trust_basis_contract_diff_report_ordering_is_frozen` | Locks level order, summary counters, and sorted report sections. |

## Future Split Targets

- `trust_basis/types.rs`: claim enums, claim/diff structs, schema constants.
- `trust_basis/diff.rs`: diff indexing, ranking, sorting, duplicate handling.
- `trust_basis/generate.rs`: bundle loading, lint integration, claim vector construction.
- `trust_basis/classifiers.rs`: signing/provenance/delegation/auth/degradation/receipt/pack classifiers.
- `trust_basis/canonical.rs`: canonical JSON serialization if it grows beyond a thin helper.

## Reviewer Focus

- This PR should not change behavior.
- The added tests are deliberately snapshot-like because Trust Basis JSON is a protocol artifact.
- Step 9 should be mechanical and keep `assay_evidence::trust_basis::*` and root re-exports stable.
