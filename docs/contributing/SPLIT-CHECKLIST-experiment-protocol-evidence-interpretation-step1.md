# SPLIT CHECKLIST - Protocol Evidence Interpretation Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/architecture/PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md`
  - `docs/contributing/SPLIT-PLAN-experiment-protocol-evidence-interpretation.md`
  - `docs/contributing/SPLIT-CHECKLIST-experiment-protocol-evidence-interpretation-step1.md`
  - `scripts/ci/review-experiment-protocol-evidence-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No `crates/` changes

## Experiment contract freeze
- [ ] Overarching invariant is explicit (no weaker classification than canonical)
- [ ] Trust signal classification (verified / self-reported / inferred) defined
- [ ] All 4 vectors have:
  - realism class label
  - clean (canonical) read
  - attack read
  - payload definition
  - success/failure criteria
- [ ] V2 vs V3 distinction is explicit (precedence error vs trust signal suppression)
- [ ] Conditions A/B/C are explicit
  - B = precedence-aware, trust-incomplete
  - C = full consumer hardening
- [ ] Metrics defined: CDR, PIR, CFR, PLR, CCAR, FPBR
- [ ] Hypotheses H1-H4 are falsifiable
- [ ] Benign controls E1/E2/E3 are explicit
- [ ] Result output shape frozen (JSON)
- [ ] Success taxonomy frozen (5 levels)

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-experiment-protocol-evidence-step1.sh` passes
- [ ] `cargo fmt --check` passes
