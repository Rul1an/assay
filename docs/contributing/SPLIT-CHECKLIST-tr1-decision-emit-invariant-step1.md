# T-R1 Step1 Checklist — `decision_emit_invariant`

## Intent

Freeze the split boundaries for `crates/assay-core/tests/decision_emit_invariant.rs` before any
mechanical module moves.

## Scope

- `docs/contributing/SPLIT-PLAN-tr1-decision-emit-invariant.md`
- `docs/contributing/SPLIT-CHECKLIST-tr1-decision-emit-invariant-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr1-decision-emit-invariant-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr1-decision-emit-invariant-step1.md`
- `scripts/ci/review-tr1-decision-emit-invariant-step1.sh`

## Step1 constraints

- docs/gates only
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/**`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no workflow edits
- no event-shape, reason-code, or emitted JSON contract drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr1-decision-emit-invariant-step1.sh
```

## Reviewer quick scan

1. Confirm the diff is limited to the 5 Step1 files.
2. Confirm `crates/assay-core/tests/**` and `crates/assay-core/src/mcp/**` remain untouched.
3. Confirm the plan freezes one coherent integration target, not multiple top-level test binaries.
4. Confirm the move-map previews `tests/decision_emit_invariant/main.rs` plus family modules.
5. Confirm the reviewer script re-runs pinned emitted-contract and delegation/auth invariants.
