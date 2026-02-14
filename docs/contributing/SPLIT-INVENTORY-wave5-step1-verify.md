# Wave5 Step1 inventory (verify behavior freeze)

Scope:
- `crates/assay-registry/src/verify.rs`

Snapshot:
- Snapshot commit (at inventory capture): `f64390b4`
- Base for Step1 drift checks: `origin/main`

LOC:
- `crates/assay-registry/src/verify.rs`: `1065`

Drift counters (best-effort code-only, tests/comments filtered):
- `unwrap/expect`: `1`
- `unsafe`: `0`
- `println/eprintln`: `0`
- `panic/todo/unimplemented`: `0`
- `dbg/trace/debug`: `0`

Contract-freeze test anchors:
- `test_verify_pack_fail_closed_matrix_contract`
- `test_verify_pack_malformed_signature_reason_is_stable`
- `test_verify_pack_canonicalization_equivalent_yaml_variants_contract`
- `test_verify_pack_uses_canonical_bytes`
- `test_verify_digest_mismatch`
- `test_parse_dsse_envelope_invalid_base64`

Notes:
- Step1 is tests/docs/gates only; no production-body changes allowed in `verify.rs`.
- Public verify API surface is frozen via symbol-diff gate in reviewer script.
