# SPLIT CHECKLIST — ADR-025 P3 Index (PR-C closure)

## Scope (hard)
- [ ] Only P3-C allowlist files changed
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes (docs + reviewer gate only)

## Index coverage contract
- [ ] `docs/architecture/ADR-025-INDEX.md` still includes I1/I2/I3 sections
- [ ] Index still links core nightly workflows and release wiring paths
- [ ] Index still lists reviewer gates for I1/I2/I3 and stabilization slices

## Status sync contract
- [ ] `docs/ROADMAP.md` contains ADR-025 P3 consolidation status sync line
- [ ] `docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md` Step4C status wording reflects merged reality on `main`

## Reviewer gate
- [ ] `scripts/ci/review-adr025-p3-index-c.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate validates index invariants and P3 A/B gate presence
