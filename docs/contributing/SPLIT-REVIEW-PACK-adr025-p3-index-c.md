# Review Pack — ADR-025 P3 Index (PR-C closure)

## Intent
Close ADR-025 P3 index consolidation by:
- adding closure checklist/review-pack artifacts
- adding a strict P3-C reviewer gate
- syncing ROADMAP/PLAN wording to current merged ADR-025 state

## Non-goals
- no workflow edits
- no runtime/script behavior changes outside reviewer gate
- no PR required-check / branch-protection changes

## Scope (allowlist)
- docs/architecture/ADR-025-INDEX.md
- docs/contributing/SPLIT-CHECKLIST-adr025-p3-index-c.md
- docs/contributing/SPLIT-REVIEW-PACK-adr025-p3-index-c.md
- docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md
- docs/ROADMAP.md
- scripts/ci/review-adr025-p3-index-c.sh

## Verification
- `BASE_REF=origin/main bash scripts/ci/review-adr025-p3-index-c.sh`
- `test -f scripts/ci/review-adr025-p3-index-a.sh`
- `test -f scripts/ci/review-adr025-p3-index-b.sh`
- `cargo fmt --check`
- `cargo clippy -p assay-cli -- -D warnings`

## Reviewer 60s scan
1) confirm no workflow files changed
2) run P3-C reviewer gate script
3) skim index status sync + roadmap/plan status lines
4) confirm index still links I1/I2/I3 core artifacts and gates
