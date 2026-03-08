# SPLIT CHECKLIST — Wave19 Coverage Command Step3 (closure)

## Scope discipline
- [ ] Step3 wijzigt alleen:
  - `docs/contributing/SPLIT-CHECKLIST-coverage-command-step3.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step3.md`
  - `scripts/ci/review-coverage-command-step3.sh`
- [ ] Geen `.github/workflows/*` wijzigingen
- [ ] Geen codewijzigingen onder `crates/assay-cli/src/cli/commands/coverage*`
- [ ] Geen scope-leaks buiten Wave19

## Closure intent
- [ ] Step3 is docs+gate only
- [ ] Step3 introduceert geen nieuwe logic
- [ ] Step3 introduceert geen nieuwe tests
- [ ] Step3 rerunt Step2 invariants zonder verruiming

## Coverage command invariants
- [ ] `cmd_coverage(...)` blijft beschikbaar via façade
- [ ] façade bevat geen write helpers
- [ ] façade bevat geen baseline/threshold bulk
- [ ] `generate.rs` draagt generator-mode markers
- [ ] `legacy.rs` draagt baseline/analyzer markers
- [ ] `io.rs` draagt write/logging helpers
- [ ] `io.rs` bevat geen schema validation / markdown render / report-build logic

## Contract tests
- [ ] `coverage_contract` blijft groen
- [ ] `coverage_out_md` blijft groen
- [ ] `coverage_declared_tools_file` blijft groen

## Promote readiness
- [ ] Step3 gate draait groen tegen stacked base
- [ ] Step3 gate kan ook groen draaien tegen `origin/main` na sync
- [ ] Closure kan promoted worden zonder extra codewijzigingen
