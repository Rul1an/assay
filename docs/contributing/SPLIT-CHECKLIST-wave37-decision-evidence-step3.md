# SPLIT CHECKLIST - Wave37 Decision Evidence Convergence Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave37-decision-evidence-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave37-decision-evidence-step3.md`
  - `scripts/ci/review-wave37-decision-evidence-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave37 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema redesign
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded decision/evidence convergence contract

## Convergence invariants
- [ ] Additive convergence fields remain present:
  - `decision_outcome_kind`
  - `decision_origin`
  - `outcome_compat_state`
- [ ] Deterministic deny classification remains present:
  - `PolicyDeny`
  - `FailClosedDeny`
  - `EnforcementDeny`
- [ ] Deterministic obligation classification remains present:
  - `ObligationApplied`
  - `ObligationSkipped`
  - `ObligationError`
- [ ] Existing fulfillment normalization remains intact:
  - `fulfillment_decision_path`
  - `obligation_applied_present`
  - `obligation_skipped_present`
  - `obligation_error_present`

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present
- [ ] `redact_args` enforcement remains present

## Validation
- [ ] Step3 gate passes against `origin/main` after sync
- [ ] Optional: Step3 gate passes against stacked Step2 base when ancestry is preserved (non-squash flow)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
