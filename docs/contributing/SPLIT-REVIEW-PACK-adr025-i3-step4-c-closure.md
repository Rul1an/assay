# Review Pack — ADR-025 I3 Step4C (closure)

## Intent
Close ADR-025 I3 Step4 by:
- adding operational runbook for OTel release integration
- capturing Step4A/4B contracts as reviewer checklist
- syncing PLAN/ROADMAP status with current main
- adding a strict Step4C reviewer gate

## Non-goals
- no workflow edits
- no runtime behavior changes in OTel release integration
- no PR-lane required-check changes

## Scope (allowlist)
- docs/ops/ADR-025-I3-OTEL-RELEASE-RUNBOOK.md
- docs/contributing/SPLIT-CHECKLIST-adr025-i3-step4-c-closure.md
- docs/contributing/SPLIT-REVIEW-PACK-adr025-i3-step4-c-closure.md
- docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md
- docs/ROADMAP.md
- scripts/ci/review-adr025-i3-step4-c.sh

## Verification
- `BASE_REF=origin/main bash scripts/ci/review-adr025-i3-step4-c.sh`
- `bash scripts/ci/review-adr025-i3-step4-a.sh`
- `bash scripts/ci/review-adr025-i3-step4-b.sh`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`

## Reviewer 60s scan
1) confirm no workflow files changed
2) run Step4C reviewer gate script
3) skim runbook for mode/exit/break-glass correctness
4) confirm PLAN/ROADMAP status lines match Step4 reality
