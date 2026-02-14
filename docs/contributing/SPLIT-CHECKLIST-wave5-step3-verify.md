# Wave5 Step3 checklist: verify closure (thin facade + final module layout)

Scope lock:
- Step3 keeps public `crate::verify::*` symbols/signatures unchanged.
- Step3 is mechanical split-finalization only (no behavior/perf changes).
- `verify.rs` remains the permanent public facade file.

Chosen decisions (explicit):
- Test location: **Option C** (dedicated internal test module).
  - Step1 anchor tests move to `crates/assay-registry/src/verify_internal/tests.rs`.
  - Reviewer script still runs the same anchor test names.
- Module migration strategy: **Plan Y** (conflict-safe).
  - Keep `crates/assay-registry/src/verify.rs` as facade.
  - Rename transitional `verify_next/*` to `verify_internal/*`.
  - Do **not** introduce `verify/mod.rs` in Step3 (avoids `verify.rs` vs `verify/mod.rs` conflict).

Artifacts:
- `docs/contributing/SPLIT-MOVE-MAP-wave5-step3-verify.md`
- `docs/contributing/SPLIT-CHECKLIST-wave5-step3-verify.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave5-step3-verify.md`
- `scripts/ci/review-wave5-step3.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave5-step3.sh
```

Precondition behavior:
- In Commit A (before module rename/move), `review-wave5-step3.sh` is expected to fail fast with:
  - missing `crates/assay-registry/src/verify_internal`
- After Commit B, the script must pass.

Hard gates (script-enforced):
- `verify.rs` facade thinness (no heavy parsing/crypto internals).
- no test module in `verify.rs` (`#[cfg(test)]` + `mod tests` forbidden).
- single-source boundaries:
  - `VerifyResult { ... }` construction only in `verify_internal/policy.rs`
  - DSSE crypto calls only in `verify_internal/dsse.rs`
  - canonicalization helpers only in `verify_internal/digest.rs` (and `verify_internal/tests.rs`)
- strict diff allowlist hard-fail:
  - `crates/assay-registry/src/verify.rs`
  - `crates/assay-registry/src/verify_internal/**`
  - `docs/contributing/SPLIT-*wave5-step3-verify*`
  - `scripts/ci/review-wave5-step3.sh`
  - `docs/architecture/PLAN-split-refactor-2026q1.md`

Definition of done:
- reviewer script passes.
- Step1 anchor tests remain green.
- move-map call chains reflect final `verify_internal/*` layout.
