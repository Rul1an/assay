# Review Pack: Wave5 Step1 (verify behavior freeze)

Intent:
- Freeze `verify.rs` behavior before the mechanical split.
- Keep production code unchanged in Step1; only test/docs/gates changes are allowed.

Scope:
- `crates/assay-registry/src/verify.rs` (tests only)
- `docs/contributing/SPLIT-INVENTORY-wave5-step1-verify.md`
- `docs/contributing/SPLIT-SYMBOLS-wave5-step1-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-verify-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step1.md`
- `scripts/ci/review-wave5-step1.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

## 1) Freeze anchors (tests + drift-sensitive asserts)

Source: `crates/assay-registry/src/verify.rs`

- `test_verify_pack_fail_closed_matrix_contract`
  - `assert!(matches!(err_unsigned_default, RegistryError::Unsigned { .. }));`
  - `assert!(matches!(err_mismatch, RegistryError::DigestMismatch { .. }));`
  - `assert!(!allowed.signed);`
- `test_verify_pack_malformed_signature_reason_is_stable`
  - `assert!(reason.starts_with("invalid base64 envelope:"));`
- `test_verify_pack_canonicalization_equivalent_yaml_variants_contract`
  - `assert_eq!(compute_digest(source_yaml), compute_digest(variant_yaml));`
  - `assert_eq!(canonicalize_for_dsse(source_yaml).unwrap(), canonicalize_for_dsse(variant_yaml).unwrap());`
- `test_verify_pack_uses_canonical_bytes`
  - `assert!(result.is_ok(), "verify_pack should canonicalize content before DSSE verification: {:?}", result);`
- `test_verify_digest_mismatch`
  - `assert!(matches!(result, Err(RegistryError::DigestMismatch { .. })));`
- `test_parse_dsse_envelope_invalid_base64`
  - `assert!(matches!(result, Err(RegistryError::SignatureInvalid { .. })));`

## 2) Public surface snapshot

Command:
```bash
rg -n "^pub (fn|struct|enum|type|const)" crates/assay-registry/src/verify.rs
```

Output:
```text
19:pub const PAYLOAD_TYPE_PACK_V1: &str = "application/vnd.assay.pack+yaml;v=1";
23:pub struct VerifyResult {
36:pub struct VerifyOptions {
72:pub fn verify_pack(
147:pub fn verify_digest(content: &str, expected: &str) -> RegistryResult<()> {
169:pub fn compute_digest(content: &str) -> String {
181:pub fn compute_digest_strict(content: &str) -> Result<String, CanonicalizeError> {
190:pub fn compute_digest_raw(content: &str) -> String {
323:pub fn compute_key_id(spki_bytes: &[u8]) -> String {
328:pub fn compute_key_id_from_key(key: &VerifyingKey) -> RegistryResult<String> {
```

## 3) Step2 hard-fail gate definitions (planned)

The following checks are planned for `scripts/ci/review-wave5-step2.sh`:

```bash
# verify.rs facade must stay thin (no heavy crypto/parsing internals)
check_no_match_code_only \
  'base64::|ed25519_dalek|serde_json::from_(slice|str)|parse_yaml_strict|to_canonical_jcs_bytes|compute_canonical_digest|build_pae\(|verify_single_signature\(' \
  crates/assay-registry/src/verify.rs

# policy.rs must stay orchestration-only (no crypto/base64/wire parsing internals)
check_no_match_code_only \
  'base64::|ed25519_dalek|serde_json::from_(slice|str)|Signature::from_slice|Verifier|build_pae\(|verify_single_signature\(' \
  crates/assay-registry/src/verify_next/policy.rs

# dsse.rs must stay policy-agnostic
check_no_match_code_only \
  'allow_unsigned|skip_signature|Unsigned|VerifyOptions|policy' \
  crates/assay-registry/src/verify_next/dsse.rs

# DSSE crypto path single-source in dsse.rs
check_only_file_matches \
  'build_pae\(|verify_single_signature\(|verify_dsse_signature_bytes_impl\(|VerifyingKey::|Signature::from_slice' \
  crates/assay-registry/src/verify_next \
  'verify_next/dsse.rs'
```

Validation command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave5-step1.sh
```
