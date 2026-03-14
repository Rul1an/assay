# SPLIT CHECKLIST - Wave40 Deny Evidence Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave40-deny-evidence-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave40-deny-evidence-step3.md`
  - `scripts/ci/review-wave40-deny-evidence-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave40 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded deny/fail-closed evidence convergence contract

## Wave40 invariants
- [ ] Deny-convergence markers remain present:
  - `policy_deny`
  - `fail_closed_deny`
  - `enforcement_deny`
  - `deny_precedence_version`
  - `deny_classification_source`
  - `deny_legacy_fallback_applied`
  - `deny_convergence_reason`
  - `DenyClassificationSource`
  - `DENY_PRECEDENCE_VERSION_V1`
  - `project_deny_convergence`
- [ ] Deterministic deny precedence markers remain present:
  - `OutcomeKind`
  - `OriginContext`
  - `FulfillmentPath`
  - `LegacyDecision`
  - `outcome_policy_deny`
  - `origin_fail_closed_matrix`
  - `fulfillment_policy_deny`
  - `legacy_policy_deny`
- [ ] Existing replay/decision markers remain present:
  - `ReplayDiffBasis`
  - `ReplayDiffBucket`
  - `DecisionOutcomeKind`
  - `OutcomeCompatState`
  - `fulfillment_decision_path`
  - `decision_basis_version`
  - `classification_source`

## Non-goals still enforced
- [ ] No runtime behavior change
- [ ] No new deny semantics
- [ ] No new obligation types
- [ ] No policy-language expansion
- [ ] No control-plane/auth transport changes

## Validation
- [ ] Step3 gate passes against stacked Step2 base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests remain green
