# SPLIT CHECKLIST - Wave8B Step1 (UCP Freeze)

## Scope

- [ ] Step1 is docs + reviewer gate only
- [ ] No `.github/workflows/*` changes
- [ ] No production code changes in `crates/assay-adapter-ucp/src/lib.rs`

## Inventory Baseline

- [ ] Hotspot confirmed: `crates/assay-adapter-ucp/src/lib.rs` (981 LOC on `origin/main`)
- [ ] Function zones inventoried (convert / parse / version / fields / mapping / payload / tests)
- [ ] Step2 target module map frozen in review pack

## Behavior Freeze

- [ ] Public surface remains unchanged (`UcpAdapter`, `ProtocolAdapter` impl)
- [ ] Event type strings unchanged
- [ ] Strict/lenient contract unchanged
- [ ] Error kind contract unchanged

## Reviewer Gate

- [ ] `scripts/ci/review-wave8b-step1.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate executes fmt + clippy + targeted UCP tests
- [ ] Gate enforces no-increase drift counters
- [ ] Gate fails when `crates/assay-adapter-ucp/src/lib.rs` changes in Step1
