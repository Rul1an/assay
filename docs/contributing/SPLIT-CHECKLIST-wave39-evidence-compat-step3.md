# SPLIT CHECKLIST - Wave39 Evidence Compat Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave39-evidence-compat-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave39-evidence-compat-step3.md`
  - `scripts/ci/review-wave39-evidence-compat-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave39 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded replay/evidence compatibility normalization contract

## Wave39 invariants
- [ ] Compatibility markers remain present:
  - `decision_basis_version`
  - `compat_fallback_applied`
  - `classification_source`
  - `replay_diff_reason`
  - `legacy_shape_detected`
  - `ReplayClassificationSource`
  - `DECISION_BASIS_VERSION_V1`
- [ ] Deterministic precedence markers remain present:
  - `ConvergedOutcome`
  - `FulfillmentPath`
  - `LegacyFallback`
  - `project_replay_compat`
  - `converged_`
  - `fulfillment_`
  - `legacy_decision_`
- [ ] Existing replay/decision markers remain present:
  - `ReplayDiffBasis`
  - `ReplayDiffBucket`
  - `classify_replay_diff`
  - `DecisionOutcomeKind`
  - `OutcomeCompatState`
  - `fulfillment_decision_path`

## Non-goals still enforced
- [ ] No runtime enforcement behavior changes
- [ ] No new obligation types
- [ ] No policy-language expansion
- [ ] No control-plane semantics
- [ ] No auth transport changes

## Validation
- [ ] Step3 gate passes against stacked Step2 base
- [ ] Step3 gate can also pass against `origin/main` after sync
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests remain green
