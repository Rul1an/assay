# SPLIT CHECKLIST - Wave35 Fulfillment Normalization Step3

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-CHECKLIST-wave35-fulfillment-normalization-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave35-fulfillment-normalization-step3.md`
  - `scripts/ci/review-wave35-fulfillment-normalization-step3.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes
- [ ] No scope leaks outside Wave35 Step3

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduces no runtime behavior changes
- [ ] Step3 introduces no schema redesign
- [ ] Step3 reruns Step2 invariants without widening scope
- [ ] Step3 preserves bounded fulfillment-normalization contract

## Fulfillment normalization invariants
- [ ] Additive normalized fields remain present:
  - `fulfillment_decision_path`
  - `obligation_applied_present`
  - `obligation_skipped_present`
  - `obligation_error_present`
- [ ] Deterministic normalization defaults remain present:
  - `obligation_applied`
  - `obligation_skipped`
  - `obligation_error`
  - `normalization_version` (`v1`)
- [ ] Deterministic decision-path mapping remains present:
  - `policy_allow`
  - `policy_deny`
  - `fail_closed_deny`
  - `decision_error`
- [ ] Policy deny vs fail-closed deny remains explicitly distinguishable

## Existing obligation line still intact
- [ ] `log` execution remains present
- [ ] `alert` execution remains present
- [ ] `approval_required` enforcement remains present
- [ ] `restrict_scope` enforcement remains present
- [ ] `redact_args` enforcement remains present

## Validation
- [ ] Step3 gate passes against `origin/main` after sync
- [ ] Optional: Step3 gate passes against stacked base when ancestry is preserved (non-squash flow)
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests remain green
