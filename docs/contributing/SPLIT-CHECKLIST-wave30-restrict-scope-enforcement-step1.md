# SPLIT CHECKLIST — Wave30 Restrict Scope Enforcement Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave30-restrict-scope-enforcement.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave30-restrict-scope-enforcement-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave30-restrict-scope-enforcement-step1.md`
  - `scripts/ci/review-wave30-restrict-scope-enforcement-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] `restrict_scope` enforcement validity checks are explicit
- [ ] Mismatch/missing/unsupported deny behavior is explicit
- [ ] Required scope evidence fields are explicit
- [ ] Non-goals are explicit (no rewrite/filter/redaction behavior)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave30-restrict-scope-enforcement-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
