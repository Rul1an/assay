# Agentic Step2 Review Pack (Mechanical Split)

## Intent

Mechanically split `agentic/mod.rs` into facade + internal modules without changing
public API or behavior.

## Scope

- `crates/assay-core/src/agentic/mod.rs`
- `crates/assay-core/src/agentic/builder.rs`
- `crates/assay-core/src/agentic/policy_helpers.rs`
- `crates/assay-core/src/agentic/tests/mod.rs`
- `docs/contributing/SPLIT-CHECKLIST-agentic-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-agentic-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-agentic-step2.md`
- `scripts/ci/review-agentic-step2.sh`

## Non-goals

- no behavior changes
- no public API changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/codex/wave12-agentic-step1-freeze bash scripts/ci/review-agentic-step2.sh
```

## Reviewer 60s scan

1. Confirm facade only keeps public surface + wrapper.
2. Confirm helper logic moved to `policy_helpers.rs`.
3. Confirm build logic moved to `builder.rs`.
4. Confirm tests moved to `tests/mod.rs` with original names retained.
5. Confirm allowlist/workflow gates pass.
