# SPLIT CHECKLIST — Wave36 Redact Args Enforcement Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave36-redact-args-enforcement.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave36-redact-args-enforcement-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave36-redact-args-enforcement-step1.md`
  - `scripts/ci/review-wave36-redact-args-enforcement-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] `redact_args` enforcement hardening scope is explicit
- [ ] frozen failure classes are explicit
- [ ] deterministic `reason_code` is explicit
- [ ] deterministic `enforcement_stage` is explicit
- [ ] deterministic `normalization_version` is explicit
- [ ] additive redact evidence fields are explicit
- [ ] non-goals are explicit (no global redact policy / no DLP / no UI/control-plane)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave36-redact-args-enforcement-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
