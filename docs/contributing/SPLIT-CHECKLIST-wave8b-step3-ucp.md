# SPLIT CHECKLIST - Wave8B Step3 (UCP Closure)

## Scope

- [ ] Step3 closure is docs + reviewer gate only
- [ ] No `.github/workflows/*` changes
- [ ] No runtime behavior changes

## Step completion

- [ ] Step1 freeze artifacts present and valid
- [ ] Step2 split artifacts present and valid
- [ ] Thin facade remains in `crates/assay-adapter-ucp/src/lib.rs`
- [ ] Internal module boundaries remain enforced

## Contract closure

- [ ] Public surface unchanged (`UcpAdapter` + `ProtocolAdapter` impl)
- [ ] Strict/lenient and error contracts unchanged
- [ ] Fixture/property tests still pass

## Reviewer gate

- [ ] `scripts/ci/review-wave8b-step3.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate re-validates UCP split invariants end-to-end
