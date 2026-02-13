# Verify split — checklist & freeze contracts

Status: Wave 1 / Step 1 (behavior freeze before mechanical split)

## Inventory snapshot (sanity, pre-split)

```bash
git rev-parse HEAD
# b702baefa7547a7ca6ad9ae5d4becc61ff38971c

wc -l crates/assay-registry/src/verify.rs
# 1065 crates/assay-registry/src/verify.rs

wc -l crates/assay-evidence/src/bundle/writer.rs
# 1442 crates/assay-evidence/src/bundle/writer.rs

rg -n "pub struct BundleWriter|verify_bundle" crates/assay-evidence/src/bundle -S
# crates/assay-evidence/src/bundle/writer.rs:122:pub struct BundleWriter<W: Write> {
# crates/assay-evidence/src/bundle/writer.rs:384:pub fn verify_bundle<R: Read>(reader: R) -> Result<VerifyResult> {
# crates/assay-evidence/src/bundle/writer.rs:693:pub fn verify_bundle_with_limits<R: Read>(reader: R, limits: VerifyLimits) -> Result<VerifyResult> {
```

## Commit-1 scope lock

- This step contains only:
  - contract checklists
  - behavior-freeze tests
  - split boundary grep-gates
- This step explicitly excludes perf/alloc optimizations.

## Behavior-freeze contracts

- `verify_pack` fail-closed matrix must stay stable:
  - unsigned strict => `RegistryError::Unsigned` (exit code `4`)
  - malformed signature => `RegistryError::SignatureInvalid` (exit code `4`)
  - digest mismatch dominates permissive options => `RegistryError::DigestMismatch` (exit code `4`)
  - `allow_unsigned=true` => success with `signed=false`
  - `skip_signature=true` => success path without DSSE parse/verify
- malformed signature reason remains deterministic for identical input.

## Boundary contract for split target

Target layout:

```text
verify/
  mod.rs
  wire.rs
  digest.rs
  dsse.rs
  keys.rs
  policy.rs
  errors.rs
```

Rules:

- `wire.rs` contains only (de)serialization types and pure parsing helpers.
- `wire.rs` contains no policy logic, no crypto verification, no base64 decode flow control.
- `policy.rs` is crypto-agnostic.
- `dsse.rs` is policy-agnostic.
- `mod.rs` is orchestration-only, no heavy internals.

## Leak-free grep gates (for split PR)

`verify/mod.rs` no heavy deps:

```bash
rg "base64::|sha2::|ed25519|rsa|serde_json::from_str" crates/assay-registry/src/verify/mod.rs
# Expect: 0
```

`verify/policy.rs` no IO/network:

```bash
rg "std::fs|tokio::fs|reqwest|Url|http::|std::net" crates/assay-registry/src/verify/policy.rs
# Expect: 0
```

`verify/dsse.rs` no policy decisions:

```bash
rg "allow_unsigned|skip_signature|policy|decision" crates/assay-registry/src/verify/dsse.rs
# Expect: 0
```
