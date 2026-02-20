# Review Pack — ADR-025 I2 Step4C (closure)

## Intent
Close ADR-025 I2 Step4 by:
- Adding operational runbook for closure release integration
- Capturing Step4A/4B contracts as reviewer checklist
- Syncing Stab B hardening semantics (debug output, violations type contract, test-mode notes)
- Syncing PLAN/ROADMAP with what is on `main`
- Adding a strict reviewer gate for this closure slice

## Non-goals
- No workflow edits
- No changes to closure generation or release runtime behavior
- No PR-lane required-check changes

## Scope (allowlist)
- docs/ops/ADR-025-I2-CLOSURE-RELEASE-RUNBOOK.md
- docs/contributing/SPLIT-CHECKLIST-adr025-i2-step4-c-closure.md
- docs/contributing/SPLIT-REVIEW-PACK-adr025-i2-step4-c-closure.md
- docs/architecture/PLAN-ADR-025-I2-audit-kit-closure-2026q2.md
- docs/ROADMAP.md
- scripts/ci/review-adr025-i2-step4-c.sh

## Verification
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i2-step4-c.sh`
- `bash scripts/ci/review-adr025-i2-step4-a.sh`
- `bash scripts/ci/review-adr025-i2-step4-b.sh`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`

## Reviewer 60s scan
1) Confirm no workflow files changed
2) Run Step4C reviewer gate script
3) Skim runbook for mode/exit/break-glass correctness
4) Confirm PLAN/ROADMAP status lines match Step4 reality
