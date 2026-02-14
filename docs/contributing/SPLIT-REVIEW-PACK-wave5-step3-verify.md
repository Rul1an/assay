# Review Pack: Wave5 Step3 (verify closure)

Intent:
- close Wave5 verify split by making `verify.rs` truly facade-only.
- finalize internal module layout with conflict-safe naming (`verify_internal/*`).

Why this strategy:
- avoids Rust module conflict (`verify.rs` vs `verify/mod.rs`).
- keeps public paths stable (`crate::verify::*`).
- keeps diff mechanical and rollbackable.

Explicit decisions:
- Test location: `crates/assay-registry/src/verify_internal/tests.rs`.
- Migration strategy: keep `verify.rs` permanent, rename `verify_next/*` -> `verify_internal/*`.

Scope allowlist (hard-fail):
- `crates/assay-registry/src/verify.rs`
- `crates/assay-registry/src/verify_internal/**`
- `docs/contributing/SPLIT-MOVE-MAP-wave5-step3-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-wave5-step3-verify.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step3-verify.md`
- `scripts/ci/review-wave5-step3.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Hard gates in `review-wave5-step3.sh`:
- BASE_REF resolve guard + effective SHA print.
- no tests in `verify.rs`.
- facade thinness ban (`base64`, `serde_json::from_*`, canonicalize internals, low-level DSSE helpers).
- single-source boundaries:
  - `VerifyResult { ... }` only in `verify_internal/policy.rs`.
  - DSSE crypto calls only in `verify_internal/dsse.rs`.
  - canonicalization helpers only in `verify_internal/digest.rs` (and `verify_internal/tests.rs`).
- strict diff allowlist fail-fast.

Validation command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave5-step3.sh
```

Expected behavior by commit:
- Commit A: expected fail-fast (precondition missing `verify_internal/*`).
- Commit B/C: expected PASS.
