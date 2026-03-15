# SPLIT CHECKLIST - Experiment Memory Poison Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/architecture/PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md`
  - `docs/contributing/SPLIT-PLAN-experiment-memory-poison.md`
  - `docs/contributing/SPLIT-CHECKLIST-experiment-memory-poison-step1.md`
  - `scripts/ci/review-experiment-memory-poison-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No `crates/` changes

## Experiment contract freeze
- [ ] Overarching invariant is explicit
- [ ] All 4 vectors have:
  - clean baseline definition
  - poisoned payload definition
  - trigger condition
  - success/failure criteria
- [ ] Poison is defined as schema-valid + internally consistent
- [ ] Conditions A/B/C are explicit and scoped
- [ ] Metrics (PRR/DASR/PPI/RDCS/FPBR) are defined
- [ ] Hypotheses H1-H4 are falsifiable
- [ ] Benign controls B1/B2/B3 are explicit
- [ ] Result output shape is frozen (JSON)
- [ ] Success taxonomy is frozen (5 levels)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-experiment-memory-poison-step1.sh` passes
- [ ] `cargo fmt --check` passes
