# SPLIT CHECKLIST - Wave42 Context Envelope Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave42-context-envelope-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave42-context-envelope-step3.md`
  - `scripts/ci/review-wave42-context-envelope-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave42 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema changes
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded context-envelope hardening contract

## Wave42 invariants
- [ ] Context-envelope markers remain present:
  - `decision_context_contract_version`
  - `context_payload_state`
  - `required_context_fields`
  - `missing_context_fields`
  - `ContextPayloadState`
  - `DECISION_CONTEXT_CONTRACT_VERSION_V1`
  - `project_context_contract`
- [ ] Deterministic context completeness markers remain present:
  - `CompleteEnvelope`
  - `PartialEnvelope`
  - `AbsentEnvelope`
  - `lane`
  - `principal`
  - `auth_context_summary`
  - `approval_state`
- [ ] Existing replay/decision markers remain present:
  - `DecisionOutcomeKind`
  - `OutcomeCompatState`
  - `fulfillment_decision_path`
  - `decision_consumer_contract_version`
  - `consumer_payload_state`

## Non-goals still enforced
- [ ] No runtime behavior change
- [ ] No new policy-engine/control-plane/auth transport scope
- [ ] No enforcement semantics changes
- [ ] No workflow changes

## Validation
- [ ] Step3 gate passes against stacked Step2 base **when that ref is synced to current main history**
- [ ] Step3 gate passes against `origin/main` after sync (authoritative closure validation)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned replay/decision tests remain green
