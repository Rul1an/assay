# SPLIT CHECKLIST — Wave35 Fulfillment Normalization Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave35-obligation-fulfillment-normalization.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave35-fulfillment-normalization-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave35-fulfillment-normalization-step1.md`
  - `scripts/ci/review-wave35-fulfillment-normalization-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Normalized `obligation_outcomes` shape is explicit
- [ ] Deterministic `reason_code` requirement is explicit
- [ ] Deterministic `enforcement_stage` requirement is explicit
- [ ] Fixed `normalization_version` requirement is explicit
- [ ] Separation model is explicit:
  - `policy_deny`
  - `fail_closed_deny`
  - `obligation_skipped`
  - `obligation_applied`
  - `obligation_error`
- [ ] Non-goals are explicit

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave35-fulfillment-normalization-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
