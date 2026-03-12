# SPLIT CHECKLIST — Wave31 Redact Args Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave31-redact-args.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave31-redact-args-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave31-redact-args-step1.md`
  - `scripts/ci/review-wave31-redact-args-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No MCP runtime code changes
- [ ] No CLI/runtime consumer changes
- [ ] No MCP server changes

## Contract freeze
- [ ] Typed `redact_args` shape is explicit
- [ ] Redactable argument zones are explicit
- [ ] Additive redaction evidence fields are explicit
- [ ] Contract-only semantics are explicit
- [ ] Non-goals are explicit (no runtime redaction execution)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave31-redact-args-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned runtime/event tests pass
