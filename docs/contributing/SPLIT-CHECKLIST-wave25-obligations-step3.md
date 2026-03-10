# SPLIT CHECKLIST - Wave25 Obligations Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave25-obligations-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave25-obligations-step3.md`
  - `scripts/ci/review-wave25-obligations-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope

## Wave25 invariants
- [ ] `allow_with_obligations` marker remains present
- [ ] `execute_log_only` marker remains present
- [ ] `legacy_warning` compatibility marker remains present
- [ ] `obligation_outcomes` marker remains present
- [ ] Outcome status markers remain present:
  - `applied`
  - `skipped`
  - `error`

## Non-goals still enforced
- [ ] No `approval_required` execution
- [ ] No `restrict_scope` execution
- [ ] No `redact_args` execution

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests remain green
