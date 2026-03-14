# SPLIT CHECKLIST - Wave42 Context Envelope Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave42-context-envelope-hardening.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave42-context-envelope-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave42-context-envelope-step1.md`
  - `scripts/ci/review-wave42-context-envelope-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Context payload surfaces are explicit:
  - `DecisionEvent`
  - `DecisionData`
- [ ] Core context fields are explicit:
  - `lane`
  - `principal`
  - `auth_context_summary`
  - `approval_state`
- [ ] Deterministic completeness semantics are explicit
- [ ] Additive context compatibility expectations are explicit
- [ ] Non-goals are explicit (no runtime behavior change)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave42-context-envelope-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests pass
