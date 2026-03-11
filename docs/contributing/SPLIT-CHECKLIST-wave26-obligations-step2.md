# SPLIT CHECKLIST - Wave26 Obligations Step2

## Scope discipline
- [ ] Step2 contains bounded runtime implementation for obligations `log` + `alert`
- [ ] No `.github/workflows/*` changes
- [ ] No auth transport changes
- [ ] No policy backend architecture changes
- [ ] No lane/control-plane expansion

## Runtime contract checks
- [ ] `alert` obligations execute in the same bounded path as `log`
- [ ] `legacy_warning` compatibility path still maps to `log`
- [ ] Unknown obligation types are non-blocking and marked `skipped`
- [ ] Existing allow/deny semantics remain stable

## Event contract checks
- [ ] Decision events keep additive `obligation_outcomes`
- [ ] Existing Wave24/Wave25 fields remain intact
- [ ] Existing event consumers remain compatible

## Non-goals enforced
- [ ] No `approval_required` execution
- [ ] No `restrict_scope` execution
- [ ] No `redact_args` execution
- [ ] No external incident/case-management integration

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave26-obligations-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests remain green
