# Wave5 Step1 checklist: verify.rs

Scope lock:
- Step1 is behavior freeze only.
- No module split in this step.
- No behavior/perf changes.
- No dependency/Cargo changes.
- `verify.rs` production body must not change in Step1.
- `verify.rs` changes are limited to `#[cfg(test)]` contract tests.

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave5-step1-verify.md`
- `docs/contributing/SPLIT-SYMBOLS-wave5-step1-verify.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step1.md`
- `scripts/ci/review-wave5-step1.sh`

Contract anchors:
- `test_verify_pack_fail_closed_matrix_contract`
- `test_verify_pack_malformed_signature_reason_is_stable`
- `test_verify_pack_canonicalization_equivalent_yaml_variants_contract`
- `test_verify_pack_uses_canonical_bytes`
- `test_verify_digest_mismatch`
- `test_parse_dsse_envelope_invalid_base64`

Drift gates (best-effort code-only):
- no increase in `unwrap/expect`
- no increase in `unsafe`
- no increase in `println/eprintln`
- no increase in `panic/todo/unimplemented`
- no increase in `dbg/trace/debug`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave5-step1.sh
```

Optional override:
```bash
BASE_REF=<your-base-ref> bash scripts/ci/review-wave5-step1.sh
```

Definition of done:
- Freeze test subset green.
- No-production-change gate green for `verify.rs`.
- Public-surface symbol gate green.
- Drift counters unchanged.
- Diff stays within allowlist.
