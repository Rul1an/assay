# Verify Step 2 Move Map (Commit B)

Status: mechanical function move with facade signatures unchanged.

## Public API (path/signature unchanged in `src/verify.rs`)

- `pub fn verify_pack(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/policy.rs`
- `pub fn verify_digest(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/digest.rs`
- `pub fn compute_digest(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/digest.rs`
- `pub fn compute_digest_strict(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/digest.rs`
- `pub fn compute_digest_raw(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/digest.rs`
- `pub fn compute_key_id(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/keys.rs`
- `pub fn compute_key_id_from_key(...)` -> facade in `src/verify.rs`, impl in `src/verify_next/keys.rs`

## Internal helpers moved

- `canonicalize_for_dsse` -> `src/verify_next/wire.rs` (`canonicalize_for_dsse_impl`)
- `parse_dsse_envelope` -> `src/verify_next/wire.rs` (`parse_dsse_envelope_impl`)
- `build_pae` -> `src/verify_next/dsse.rs` (`build_pae_impl`)
- `verify_dsse_signature_bytes` -> `src/verify_next/dsse.rs` (`verify_dsse_signature_bytes_impl`)
- `verify_single_signature` -> `src/verify_next/dsse.rs` (`verify_single_signature_impl`)

## Error construction moved

- common error constructors are centralized in `src/verify_next/errors.rs`

## Boundary intent

- `policy.rs`: allow/skip/unsigned decision logic only
- `dsse.rs`: crypto verify only (no policy decisions)
- `wire.rs`: parsing and shape validation only
- `digest.rs`: digest-only logic
