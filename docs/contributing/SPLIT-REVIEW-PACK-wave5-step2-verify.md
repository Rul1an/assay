# Review Pack: Wave5 Step2 (verify mechanical split)

Intent:
- Mechanically split `verify.rs` internals into `verify_next/*`.
- Keep public verify surface stable via facade delegation.

Scope:
- `crates/assay-registry/src/verify.rs`
- `crates/assay-registry/src/verify_next/*`
- `docs/contributing/SPLIT-MOVE-MAP-wave5-step2-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-wave5-step2-verify.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step2-verify.md`
- `scripts/ci/review-wave5-step2.sh`

Core proof points:
- Public surface remains in `verify.rs` (same symbols/signatures as Step1 snapshot).
- Public helper bodies delegate to `verify_next/*` (including digest/key helpers).
- Fail-closed/canonicalization anchors from Step1 remain green.

Gates enforced by `review-wave5-step2.sh`:
- Delegation callsite presence checks in `verify.rs`.
- `verify_next::` path leak check outside facade.
- Facade heavy-internal ban (`base64`, `serde_json::from_*`, canonicalize internals, DSSE low-level helpers).
- `policy.rs` low-level crypto-ban + exact one DSSE boundary call.
- `dsse.rs` policy-token ban.
- DSSE crypto helper single-source gate in `verify_next/dsse.rs`.
- Diff allowlist gate.

Validation command:
```bash
BASE_REF=origin/codex/wave5-step1-verify-freeze bash scripts/ci/review-wave5-step2.sh
```

Expected outcome:
- PASS with no boundary-gate violations.
