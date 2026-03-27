# T-R2 Step1 Checklist — `tool_call_handler/tests.rs`

## Intent

Freeze the split boundaries for `crates/assay-core/src/mcp/tool_call_handler/tests.rs` before any
mechanical unit-test module moves.

## Scope

- `docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md`
- `docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step1.md`
- `scripts/ci/review-tr2-tool-call-handler-tests-step1.sh`

## Step1 constraints

- docs/gates only
- no edits under `crates/assay-core/src/mcp/tool_call_handler/**`
- no edits under `crates/assay-core/tests/**`
- no edits under `crates/assay-core/src/mcp/decision.rs`
- no edits under `crates/assay-core/src/mcp/policy/**`
- no workflow edits
- no handler behavior, private-access coverage, or emitted-decision coupling drift

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-tr2-tool-call-handler-tests-step1.sh
```

## Reviewer quick scan

1. Confirm the diff is limited to the 5 Step1 files.
2. Confirm `crates/assay-core/src/mcp/tool_call_handler/**` stays untouched in this freeze wave.
3. Confirm the plan preserves a `src`-local unit-test tree rather than introducing an integration target.
4. Confirm `tests/mod.rs` is explicitly planned as a thin module root and `fixtures.rs` as shared helpers only.
5. Confirm the reviewer script re-runs emission, delegation, approval, scope, redaction, tool-drift, classification, and lifecycle anchors.
