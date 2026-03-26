# Wave45 Policy Engine Step1 Checklist (Freeze)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave45-policy-engine.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step1.md`
  - `scripts/ci/review-wave45-policy-engine-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-core/src/mcp/policy/**`
- [ ] No edits under `crates/assay-core/tests/**`
- [ ] No handler / decision / evidence / CLI changes

## Frozen contract

- [ ] `McpPolicy::{evaluate,evaluate_with_metadata,check}` are explicitly frozen as the stable facade
- [ ] No allow/deny outcome drift is allowed in Step2
- [ ] No precedence / specificity drift is allowed in Step2
- [ ] No default / fail-closed drift is allowed in Step2
- [ ] No reason-code or policy-code drift is allowed in Step2
- [ ] No downstream decision-event metadata drift is allowed in Step2
- [ ] Step2 non-goals explicitly forbid policy-language redesign, optimization, or test reorganization

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave45-policy-engine-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core --all-targets -- -D warnings` passes
- [ ] Pinned policy/taxonomy/decision-event invariants pass
