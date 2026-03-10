# SPLIT CHECKLIST - Wave25 Obligations Step2

## Scope discipline
- [ ] Step2 contains bounded runtime implementation for obligations `log` only
- [ ] No `.github/workflows/*` changes
- [ ] No auth transport changes
- [ ] No policy backend architecture changes
- [ ] No lane/control-plane expansion

## Runtime contract checks
- [ ] `allow_with_obligations` decisions produce runtime obligation outcomes
- [ ] `legacy_warning` compatibility path maps to `log` outcome
- [ ] Unknown obligation types are non-blocking and marked `skipped`
- [ ] Existing allow/deny semantics remain stable

## Event contract checks
- [ ] Decision event adds additive `obligation_outcomes`
- [ ] Existing Wave24 Decision Event v2 fields remain intact
- [ ] Existing event consumers remain compatible

## Non-goals enforced
- [ ] No `approval_required` execution
- [ ] No `restrict_scope` execution
- [ ] No `redact_args` execution

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave25-obligations-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests remain green
