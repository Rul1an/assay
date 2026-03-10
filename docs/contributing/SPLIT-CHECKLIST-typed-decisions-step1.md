# SPLIT CHECKLIST — Wave24 Typed Decisions Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave24-typed-decisions.md`
  - `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step1.md`
  - `scripts/ci/review-wave24-typed-decisions-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No code changes under MCP/core/CLI/server paths
- [ ] No untracked files under frozen runtime paths

## Contract freeze
- [ ] Typed decision target model is frozen:
  - `allow`
  - `allow_with_obligations`
  - `deny`
  - `deny_with_alert`
- [ ] `AllowWithWarning` compatibility rule is explicit
- [ ] Decision Event v2 fields are explicitly listed
- [ ] Existing event fields are explicitly preserved
- [ ] No obligations execution is included in this wave

## Validation
- [ ] Step1 review script passes against `origin/main`
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass

## Reviewer expectations
- [ ] Freeze only
- [ ] No runtime changes
- [ ] No schema implementation changes
- [ ] No transport/auth scope expansion
