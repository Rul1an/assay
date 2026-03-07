# Tool Call Handler Step3 Review Pack (Closure)

## Intent

Close Wave16 tool-call-handler split with a docs+gate-only slice while preserving all Step2 invariants.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step3.md`
- `scripts/ci/review-tool-call-handler-step3.sh`

## Non-goals

- no code changes under `crates/assay-core/src/mcp/tool_call_handler/**`
- no workflow changes

## Validation

```bash
BASE_REF=origin/codex/wave16-tool-call-handler-step2-mechanical bash scripts/ci/review-tool-call-handler-step3.sh
```

## Reviewer 60s scan

1. Confirm Step3 diff is docs/script only.
2. Confirm Step3 gate re-runs Step2 quality checks.
3. Confirm Step3 gate re-runs Step2 facade and boundary invariants.
4. Confirm no `.github/workflows/*` changes.
