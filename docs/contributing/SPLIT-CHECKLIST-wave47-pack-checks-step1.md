# Wave47 Pack Checks Step1 Checklist (Freeze)

## Scope lock

- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-wave47-pack-checks.md`
  - `docs/contributing/SPLIT-CHECKLIST-wave47-pack-checks-step1.md`
  - `docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-wave47-pack-checks-step1.md`
  - `scripts/ci/review-wave47-pack-checks-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No edits under `crates/assay-evidence/src/lint/packs/**`
- [ ] No edits under `crates/assay-evidence/tests/**`
- [ ] No edits under `packs/open/**`

## Frozen contract

- [ ] `CheckContext`, `CheckResult`, `ENGINE_VERSION`, and `execute_check` are explicitly frozen as the stable facade
- [ ] No check-dispatch drift is allowed in Step2
- [ ] No `json_path_exists` / `value_equals` runtime drift is allowed in Step2
- [ ] No single-path `value_equals` invariant drift is allowed in Step2
- [ ] No `event_type_exists`, `event_field_present`, `conditional`, or `g3_authorization_context_present` scoped-event drift is allowed in Step2
- [ ] No finding severity / rule-id / explanation coupling drift is allowed in Step2
- [ ] No built-in/open parity drift is allowed in Step2
- [ ] Step2 non-goals explicitly forbid engine bumps, spec expansion, new check kinds, and test reorganization

## Validation

- [ ] `BASE_REF=origin/main bash scripts/ci/review-wave47-pack-checks-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-evidence --all-targets -- -D warnings` passes
- [ ] Pinned execution / parity invariants pass
