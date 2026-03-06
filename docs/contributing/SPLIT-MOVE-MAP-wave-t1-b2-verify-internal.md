# SPLIT MOVE MAP - Wave T1 B2 (verify_internal tests)

## Section Move Map

- `tests.rs` imports + helper wrappers -> `tests/mod.rs`
- Digest/hash/PAE/header-size contracts -> `tests/digest.rs`
- DSSE signature vectors and key trust contracts -> `tests/dsse.rs`
- Verify-pack fail-closed/reason-stability contracts -> `tests/failures.rs`
- Canonical-bytes/provenance parity contracts -> `tests/provenance.rs`

## Symbol Anchors

- Shared helpers in `tests/mod.rs`:
  - `canonicalize_for_dsse`
  - `parse_dsse_envelope`
  - `build_pae`
  - `verify_dsse_signature_bytes`
  - `generate_keypair`
  - `verify_dsse_signature_legacy_for_tests`
  - `keypair_from_seed`
  - `create_signed_envelope`
  - `make_fetch_result`

## Contract Targets (unchanged)

- `test_verify_pack_fail_closed_matrix_contract`
- `test_verify_pack_malformed_signature_reason_is_stable`
- `test_verify_pack_uses_canonical_bytes`
- `test_verify_pack_canonicalization_equivalent_yaml_variants_contract`
