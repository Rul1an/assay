# Wave7 Step3 checklist: runtime authz closure (thin facade)

Scope lock:
- Step3 keeps public runtime authz API unchanged.
- Step3 finalizes closure semantics for `authorizer`:
  - `authorizer_next/*` renamed to `authorizer_internal/*`
  - tests moved off `authorizer.rs` facade into `authorizer_internal/tests.rs`
- `authorizer.rs` remains stable public facade file.

Artifacts:
- `docs/contributing/SPLIT-CHECKLIST-wave7-step3-runtime-authz.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave7-step3-runtime-authz.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave7-step3-runtime-authz.md`
- `scripts/ci/review-wave7-step3.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave7-step3.sh
```

Hard gates (script-enforced):
- `cargo fmt --check`
- `cargo clippy -p assay-core --all-targets -- -D warnings`
- `cargo check -p assay-core`
- Step1 runtime authz anchor tests remain green.
- No tests in facade:
  - forbid `#[cfg(test)]`
  - forbid `mod tests {` and `mod tests;`
- Facade thinness (`authorizer.rs`):
  - forbid store/txn/IO/process internals
  - require explicit delegation calls for all public entrypoints.
- Single-source boundaries:
  - policy/store orchestration calls only in `authorizer_internal/run.rs`
  - store mutation/read boundary only in `authorizer_internal/store.rs`
  - transaction-ref hash/canonicalization helper only in `authorizer_internal/policy.rs` (+ tests).
  - mandate txn SQL boundary remains only in `mandate_store_next/txn.rs`.
- Strict diff allowlist.

Definition of done:
- reviewer script passes
- `authorizer.rs` is testless and delegation-thin
- `authorizer_internal/*` owns implementation boundaries
