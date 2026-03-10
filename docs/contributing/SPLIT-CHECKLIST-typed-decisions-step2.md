# SPLIT CHECKLIST — Wave24 Typed Decisions Step2

## Scope discipline
- [ ] Diff blijft binnen Wave24 Step2 implementatiescope:
  - `crates/assay-core/src/mcp/**`
  - `crates/assay-core/tests/{decision_emit_invariant.rs,tool_taxonomy_policy_match.rs}`
  - optioneel compat: `crates/assay-cli/src/cli/commands/{mcp.rs,session_state_window.rs,coverage/**}`
  - optioneel server compat: `crates/assay-mcp-server/{src/auth.rs,tests/auth_integration.rs}`
  - Step2 docs + gate:
    - `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step2.md`
    - `docs/contributing/SPLIT-MOVE-MAP-typed-decisions-step2.md`
    - `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step2.md`
    - `scripts/ci/review-wave24-typed-decisions-step2.sh`
- [ ] No `.github/workflows/*` changes
- [ ] Geen scope leaks buiten Wave24 Step2

## Contract invariants
- [ ] Typed decision markers aanwezig:
  - `allow_with_obligations`
  - `deny_with_alert`
- [ ] `AllowWithWarning` compat-path blijft aanwezig
- [ ] Decision Event v2 markers aanwezig:
  - `policy_version`
  - `policy_digest`
  - `obligations`
  - `approval_state`
  - `lane`
  - `principal`
  - `auth_context_summary`
- [ ] Legacy eventvelden blijven aanwezig:
  - `tool_classes`
  - `matched_tool_classes`
  - `match_basis`
  - `matched_rule`
  - `reason_code`

## Non-goals in Step2
- [ ] No obligations execution toegevoegd
- [ ] No approval enforcement toegevoegd
- [ ] No backend swap (Cedar/OPA) toegevoegd
- [ ] No auth transport model wijzigingen toegevoegd

## Validation
- [ ] `BASE_REF=origin/codex/wave24-typed-decisions-step1-freeze bash scripts/ci/review-wave24-typed-decisions-step2.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings` passes
- [ ] Pinned tests blijven groen
