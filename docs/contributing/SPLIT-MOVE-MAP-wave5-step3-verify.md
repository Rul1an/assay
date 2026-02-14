# Wave5 Step3 move map: verify closure

Goal:
- finalize Wave5 verify split with a thin permanent facade (`verify.rs`) and conflict-safe internal layout (`verify_internal/*`).

Module responsibilities (final Step3 target):
- `verify.rs`: public surface + delegation only
- `verify_internal/policy.rs`: verification orchestration + fail-closed policy only
- `verify_internal/digest.rs`: canonicalization + digest compute/compare only
- `verify_internal/wire.rs`: DSSE envelope wire parsing only
- `verify_internal/dsse.rs`: PAE + signature verification only
- `verify_internal/keys.rs`: key-id helper functions only
- `verify_internal/errors.rs`: internal error constructors/helpers only
- `verify_internal/tests.rs`: moved Step1/Step2 verify anchor tests

Entry-point call chains (final target):
- `verify_pack`
  - `verify.rs::verify_pack`
  - -> `verify_internal::policy::verify_pack_impl`
  - -> `verify_internal::wire::parse_dsse_envelope_impl`
  - -> `verify_internal::dsse::verify_dsse_signature_impl`
  - -> `verify_internal::digest::canonicalize_for_dsse_impl`
  - -> `verify_internal::dsse::verify_dsse_signature_bytes_impl`
- `verify_digest`
  - `verify.rs::verify_digest`
  - -> `verify_internal::digest::verify_digest_impl`
  - -> `verify_internal::digest::compute_digest_impl`
- `compute_digest*`
  - `verify.rs::compute_digest`
  - -> `verify_internal::digest::compute_digest_impl`
  - `verify.rs::compute_digest_strict`
  - -> `verify_internal::digest::compute_digest_strict_impl`
  - `verify.rs::compute_digest_raw`
  - -> `verify_internal::digest::compute_digest_raw_impl`
- `compute_key_id*`
  - `verify.rs::compute_key_id`
  - -> `verify_internal::keys::compute_key_id_impl`
  - `verify.rs::compute_key_id_from_key`
  - -> `verify_internal::keys::compute_key_id_from_key_impl`

Mechanical migration plan (Step3 B):
- rename folder: `verify_next/*` -> `verify_internal/*`
- update facade delegation paths in `verify.rs`
- move tests from `verify.rs` into `verify_internal/tests.rs`

Mechanics contract:
- no public symbol/signature changes in `verify.rs`
- no error classification/contract changes
- no behavior/perf changes
