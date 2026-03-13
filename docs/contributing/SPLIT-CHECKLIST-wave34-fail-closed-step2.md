# SPLIT CHECKLIST - Wave34 Fail-Closed Step2

## Scope discipline
- [ ] Diff is limited to bounded runtime + tests + Step2 docs/gate
- [ ] No `.github/workflows/*` changes
- [ ] No scope leaks outside MCP runtime/consumer paths
- [ ] No auth transport model changes
- [ ] No new obligation types added

## Implementation contract
- [ ] Typed fail-closed context is represented in code (`FailClosedContext`)
- [ ] Matrix dimensions are represented additively:
  - `tool_risk_class`
  - `fail_closed_mode`
  - `fail_closed_trigger`
  - `fail_closed_applied`
  - `fail_closed_error_code`
- [ ] Deterministic baseline codes are represented:
  - `fail_closed_context_provider_unavailable`
  - `fail_closed_runtime_dependency_error`
  - `degrade_read_only_runtime_dependency_error`
- [ ] Existing allow/deny reason-code contract remains stable

## Compatibility and behavior
- [ ] Decision/event payload remains additive and backward-compatible
- [ ] Existing obligation execution (`log`, `alert`, `approval_required`, `restrict_scope`, `redact_args`) remains intact
- [ ] No control-plane/workflow semantics added

## Validation
- [ ] `BASE_REF=origin/codex/wave34-fail-closed-matrix-step1-freeze bash scripts/ci/review-wave34-fail-closed-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
