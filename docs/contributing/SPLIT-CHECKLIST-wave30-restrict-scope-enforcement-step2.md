# SPLIT CHECKLIST — Wave30 Restrict Scope Enforcement Step2

## Scope discipline
- [ ] Only bounded runtime + tests + Step2 docs/gate files changed
- [ ] No `.github/workflows/*` changes
- [ ] No non-wave scope leaks

## Enforcement contract
- [ ] `restrict_scope` is runtime-enforced
- [ ] Missing/mismatch/unsupported scope outcomes deterministically deny
- [ ] Deny reason code is explicit (`P_RESTRICT_SCOPE`)
- [ ] Failure reasons remain deterministic:
  - `scope_target_missing`
  - `scope_target_mismatch`
  - `scope_match_mode_unsupported`
  - `scope_type_unsupported`

## Evidence compatibility
- [ ] Scope evidence fields remain additive and backward-compatible:
  - `scope_type`
  - `scope_value`
  - `scope_match_mode`
  - `scope_evaluation_state`
  - `scope_failure_reason`
  - `restrict_scope_present`
  - `restrict_scope_target`
  - `restrict_scope_match`
  - `restrict_scope_reason`
- [ ] Existing event consumers remain compatible

## Non-goals still enforced
- [ ] No argument rewriting/filtering added
- [ ] No `redact_args` execution added
- [ ] No broad/global scope semantics added
- [ ] No control-plane/auth transport work added

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave30-restrict-scope-enforcement-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
