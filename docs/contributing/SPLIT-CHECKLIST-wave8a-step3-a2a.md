# SPLIT CHECKLIST - Wave8A Step3 (A2A Closure)

## Scope

- [ ] Step3 closure is docs + reviewer gate only
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes

## Step completion

- [ ] Step1 freeze artifacts present and valid
- [ ] Step2 split artifacts present and valid
- [ ] Thin facade remains in `crates/assay-adapter-a2a/src/lib.rs`
- [ ] Internal module boundaries remain enforced

## Contract closure

- [ ] Public surface unchanged (`A2aAdapter` + `ProtocolAdapter` impl)
- [ ] Strict/lenient and error contracts unchanged
- [ ] Fixture/property tests still pass

## Reviewer gate

- [ ] `scripts/ci/review-wave8a-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate re-validates A2A split invariants end-to-end
