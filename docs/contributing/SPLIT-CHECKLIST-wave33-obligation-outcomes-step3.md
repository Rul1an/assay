# SPLIT CHECKLIST — Wave33 Obligation Outcomes Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave33-obligation-outcomes-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave33-obligation-outcomes-step3.md`
  - `scripts/ci/review-wave33-obligation-outcomes-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave33 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded obligation-outcome normalization semantics

## Normalization invariants
- [ ] `ObligationOutcome` fields remain present:
  - `obligation_type`
  - `status`
  - `reason`
  - `reason_code`
  - `enforcement_stage`
  - `normalization_version`
- [ ] Baseline reason-code markers remain present:
  - `legacy_warning_mapped`
  - `validated_in_handler`
  - `contract_only`
  - `unsupported_obligation_type`
  - `approval_missing`
  - `approval_expired`
  - `approval_bound_tool_mismatch`
  - `approval_bound_resource_mismatch`
  - `scope_target_missing`
  - `scope_target_mismatch`
  - `scope_match_mode_unsupported`
  - `scope_type_unsupported`
  - `redaction_target_missing`
  - `redaction_mode_unsupported`
  - `redaction_scope_unsupported`
  - `redaction_apply_failed`

## Behavior containment still enforced
- [ ] No allow/deny behavior changes introduced
- [ ] No new obligation execution semantics added
- [ ] Existing obligation line remains intact

## Validation
- [ ] Step3 gate passes against stacked base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
