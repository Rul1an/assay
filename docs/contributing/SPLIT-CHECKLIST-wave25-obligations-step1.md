# SPLIT CHECKLIST - Wave25 Obligations Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave25-obligations-log.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave25-obligations-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave25-obligations-step1.md`
  - `scripts/ci/review-wave25-obligations-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No runtime code changes under MCP/core/CLI/server paths
- [ ] No untracked files under frozen runtime paths

## Contract freeze
- [ ] Wave25 execution scope is frozen to obligation type `log`
- [ ] Compatibility for `legacy_warning` is explicit
- [ ] `obligation_outcomes` event addition is explicit and additive
- [ ] Outcome statuses are explicitly frozen:
  - `applied`
  - `skipped`
  - `error`
- [ ] No approval/restrict/redact execution is included in this wave

## Validation
- [ ] Step1 review script passes against `origin/main`
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass

## Reviewer expectations
- [ ] Freeze only
- [ ] No runtime implementation changes
- [ ] No policy backend/auth scope expansion
- [ ] No high-risk obligation execution added
