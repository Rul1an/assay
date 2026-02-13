# Verify split (Step 2) — checklist & grep-gates

Status: Wave 1 / Step 2 (mechanical move behind stable facade)

## Scope lock

- `src/verify.rs` remains the public facade.
- Public symbols/paths remain unchanged.
- `verify_next/*` hosts moved implementation only.
- No perf work and no API redesign in this step.

## Reviewer script (copy/paste)

```bash
cargo check -p assay-registry
cargo test -p assay-registry test_verify_pack_fail_closed_matrix_contract -- --nocapture
cargo test -p assay-registry test_verify_pack_malformed_signature_reason_is_stable -- --nocapture
```

## Boundary grep-gates (copy/paste)

### 1) policy.rs has no direct crypto/parsing/IO internals

```bash
rg -n "base64::|ed25519|sha2::|serde_json::from_slice|reqwest|tokio::fs|std::fs|std::net" crates/assay-registry/src/verify_next/policy.rs | rg -v '^\d+:\s*//!'
# Expect: empty output
```

### 2) policy.rs does not call low-level DSSE crypto helpers

```bash
rg -n "build_pae_impl|verify_single_signature_impl|Signature::|Verifier::|to_public_key_der" crates/assay-registry/src/verify_next/policy.rs | rg -v '^\d+:\s*//!'
# Expect: empty output
```

### 3) policy.rs has exactly one DSSE boundary call site

```bash
rg -n "dsse_next::" crates/assay-registry/src/verify_next/policy.rs | rg -v '^\d+:\s*//!'
# Expect: one match to dsse_next::verify_dsse_signature_bytes_impl(...)
```

### 4) dsse.rs contains no policy tokens

```bash
rg -n "allow_unsigned|skip_signature|unsigned" crates/assay-registry/src/verify_next/dsse.rs | rg -v '^\d+:\s*//!'
# Expect: empty output
```

### 5) facade path stays stable

```bash
rg -n "pub mod verify;" crates/assay-registry/src/lib.rs
# Expect: exactly one match
```

## Move-map reference

See `docs/contributing/SPLIT-MOVE-MAP-verify-step2.md` for function-to-file mapping.

## Known sandbox limitation

In this sandbox, `registry_client` integration tests that spawn wiremock can fail with
`Operation not permitted` (port bind). This does not affect Step-2 verify split
correctness. Use GitHub CI for full integration-suite confirmation.
