# SPLIT CHECKLIST - Wave34 Fail-Closed Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave34-fail-closed-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave34-fail-closed-step3.md`
  - `scripts/ci/review-wave34-fail-closed-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave34 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema redesign
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded fail-closed matrix typing contract

## Fail-closed invariants
- [ ] `FailClosedContext` remains present
- [ ] `ToolRiskClass` remains present
- [ ] `FailClosedMode` remains present
- [ ] `FailClosedTrigger` remains present
- [ ] Additive fail-closed event context remains present (`fail_closed`)
- [ ] Baseline fail-closed reason codes remain present:
  - `fail_closed_context_provider_unavailable`
  - `fail_closed_runtime_dependency_error`
  - `degrade_read_only_runtime_dependency_error`

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present
- [ ] `redact_args` enforcement remains present

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
