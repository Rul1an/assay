# SPLIT CHECKLIST - Wave32 Redact Args Enforcement Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave32-redact-args-enforcement.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave32-redact-args-enforcement-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave32-redact-args-enforcement-step1.md`
  - `scripts/ci/review-wave32-redact-args-enforcement-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] `redact_args` enforcement validity checks are explicit
- [ ] Missing/unsupported/not-applied deny behavior is explicit
- [ ] Required redaction evidence fields are explicit
- [ ] Non-goals are explicit (no broad/global redaction behavior)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave32-redact-args-enforcement-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
