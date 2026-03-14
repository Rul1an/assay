# SPLIT CHECKLIST - Wave41 Consumer Hardening Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave41-consumer-hardening-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave41-consumer-hardening-step3.md`
  - `scripts/ci/review-wave41-consumer-hardening-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave41 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves the bounded Wave41 consumer-hardening contract

## Wave41 invariants
- [ ] Consumer-hardening markers remain present:
  - `decision_consumer_contract_version`
  - `consumer_read_path`
  - `consumer_fallback_applied`
  - `consumer_payload_state`
  - `required_consumer_fields`
  - `ConsumerReadPath`
  - `ConsumerPayloadState`
  - `DECISION_CONSUMER_CONTRACT_VERSION_V1`
  - `project_consumer_contract`
- [ ] Deterministic consumer precedence markers remain present:
  - `ConvergedDecision`
  - `CompatibilityMarkers`
  - `LegacyDecision`
  - `decision_outcome_kind`
  - `classification_source`
  - `legacy_shape_detected`
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
- [ ] No new runtime capability
- [ ] No enforcement semantics change
- [ ] No policy-engine/control-plane/auth transport expansion

## Validation
- [ ] Step3 gate passes against stacked Step2 base **when that ref is synced to current main history**
- [ ] Step3 gate passes against `origin/main` after sync (authoritative closure validation)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests remain green
