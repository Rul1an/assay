# SPLIT CHECKLIST - Wave29 Restrict Scope Step2

## Scope discipline
- [ ] Step2 contains bounded `restrict_scope` contract/evidence implementation only
- [ ] No `.github/workflows/*` changes
- [ ] No runtime enforcement of `restrict_scope`
- [ ] No argument rewriting/filtering/redaction behavior
- [ ] No broad/global scope semantics
- [ ] No control-plane or policy-backend changes

## Restrict-scope shape checks
- [ ] `scope_type` is represented in runtime policy/event path
- [ ] `scope_value` is represented in runtime policy/event path
- [ ] `scope_match_mode` is represented in runtime policy/event path
- [ ] `scope_evaluation_state` is represented in runtime policy/event path
- [ ] `scope_failure_reason` is represented in runtime policy/event path

## Additive evidence checks
- [ ] `restrict_scope_present` is represented additively
- [ ] `restrict_scope_target` is represented additively
- [ ] `restrict_scope_match` is represented additively
- [ ] `restrict_scope_reason` is represented additively
- [ ] Existing decision/event consumers remain backward-compatible

## Stability checks
- [ ] Existing `log` obligation behavior remains stable
- [ ] Existing `alert` obligation behavior remains stable
- [ ] Existing `approval_required` behavior remains stable
- [ ] Existing `legacy_warning -> log` compat remains stable

## Non-goals enforced
- [ ] No `restrict_scope` deny path added
- [ ] No `restrict_scope` blocking execution behavior added
- [ ] No `redact_args` execution added
- [ ] No `restrict_scope` control-plane semantics added

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave29-restrict-scope-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned core/cli/server tests remain green
