# SPLIT CHECKLIST - Experiment Delegation Spoofing Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/architecture/PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md`
  - `docs/contributing/SPLIT-PLAN-experiment-delegation-spoofing.md`
  - `docs/contributing/SPLIT-CHECKLIST-experiment-delegation-spoofing-step1.md`
  - `scripts/ci/review-experiment-delegation-spoofing-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No `crates/` changes

## Experiment contract freeze
- [ ] Overarching invariant is explicit (no silent trust upgrade)
- [ ] All 4 vectors have:
  - clean baseline definition
  - poisoned payload definition
  - trigger condition
  - success/failure criteria
- [ ] Poison is defined as schema-valid + protocol-plausible + trust-affecting
- [ ] Conditions A/B/C are explicit and scoped
- [ ] Metrics (COR/PBR/ISSR/SMR/FPBR) are defined
- [ ] Hypotheses H1-H4 are falsifiable
- [ ] Benign controls D1/D2/D3 are explicit
- [ ] Result output shape is frozen (JSON)
- [ ] Success taxonomy is frozen (5 levels)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-experiment-delegation-spoofing-step1.sh` passes
- [ ] `cargo fmt --check` passes
