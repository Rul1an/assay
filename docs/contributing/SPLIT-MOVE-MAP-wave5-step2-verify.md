# Wave5 Step2 move map: verify

Module responsibilities:
- `verify.rs`: public surface + thin delegating facade
- `verify_next/policy.rs`: verification orchestration + fail-closed policy only
- `verify_next/digest.rs`: digest compute/compare helpers only
- `verify_next/wire.rs`: DSSE envelope wire parsing only
- `verify_next/dsse.rs`: canonicalization-for-DSSE + PAE + signature verification only
- `verify_next/keys.rs`: key-id helper functions only

Function -> file map:
- `verify_pack` -> `verify.rs` facade, impl in `verify_next/policy.rs`
- `verify_digest` -> `verify.rs` facade, impl in `verify_next/digest.rs`
- `compute_digest` -> `verify.rs` facade, impl in `verify_next/digest.rs`
- `compute_digest_strict` -> `verify.rs` facade, impl in `verify_next/digest.rs`
- `compute_digest_raw` -> `verify.rs` facade, impl in `verify_next/digest.rs`
- `parse_dsse_envelope` (test helper wrapper) -> `verify.rs`, impl in `verify_next/wire.rs`
- `canonicalize_for_dsse` (test helper wrapper) -> `verify.rs`, impl in `verify_next/dsse.rs`
- `build_pae` (test helper wrapper) -> `verify.rs`, impl in `verify_next/dsse.rs`
- `verify_dsse_signature_bytes` (test helper wrapper) -> `verify.rs`, impl in `verify_next/dsse.rs`
- `verify_single_signature_impl` -> `verify_next/dsse.rs`
- `compute_key_id` -> `verify.rs` facade, impl in `verify_next/keys.rs`
- `compute_key_id_from_key` -> `verify.rs` facade, impl in `verify_next/keys.rs`

Caller chains (top flows):
- Verify pack flow:
  - `verify_pack` -> `verify_next::policy::verify_pack_impl`
  - `verify_pack_impl` -> `canonicalize_for_dsse_impl` + `parse_dsse_envelope_impl` + `verify_dsse_signature_bytes_impl`
- Digest compare flow:
  - `verify_digest` -> `verify_next::digest::verify_digest_impl`
  - `verify_digest_impl` -> `compute_digest_impl`
- DSSE envelope flow:
  - `verify_pack_impl` -> `parse_dsse_envelope_impl` (wire)
  - `verify_dsse_signature_bytes_impl` (dsse) -> `build_pae_impl` -> `verify_single_signature_impl`

Mechanics contract:
- Step2 is mechanical move only.
- No public symbol/signature changes in `verify.rs`.
- No error-code/classification contract changes.
