# Tool Call Handler Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step3.md`
- `scripts/ci/review-tool-call-handler-step3.sh`
- no code changes in Step3
- no workflow changes

## Closure invariants

- re-run Step2 quality checks (`fmt`, `clippy`, targeted tests)
- re-run Step2 facade invariants (`mod.rs` thin wrappers)
- re-run Step2 boundary invariants (`DecisionEvent::new` only in `emit.rs`)
- re-run Step2 test relocation invariants (`tests.rs` keeps moved test names)

## Gate requirements

- allowlist-only diff vs Step2 base branch
- workflow-ban (`.github/workflows/*`)
- quality checks:
  - `cargo fmt --check`
  - `cargo clippy -p assay-core --all-targets -- -D warnings`
  - `cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact`
  - `cargo test -p assay-core test_event_contains_required_fields -- --exact`

## Definition of done

- `BASE_REF=origin/codex/wave16-tool-call-handler-step2-mechanical bash scripts/ci/review-tool-call-handler-step3.sh` passes
- Step3 diff is docs+script only
