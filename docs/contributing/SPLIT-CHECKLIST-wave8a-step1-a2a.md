# SPLIT CHECKLIST - Wave8A Step1 (A2A Freeze)

## Scope

- [ ] Step1 is docs + reviewer gate only
- [ ] No `.github/workflows/*` changes
- [ ] No production code changes in `crates/assay-adapter-a2a/src/lib.rs`

## Inventory Baseline

- [ ] Hotspot confirmed: `crates/assay-adapter-a2a/src/lib.rs` (998 LOC on `origin/main`)
- [ ] Function zones inventoried (convert / parse / version / fields / mapping / payload / tests)
- [ ] Step2 target module map is fixed in review pack

## Behavior Freeze

- [ ] Public surface remains unchanged (`A2aAdapter`, `ProtocolAdapter` impl)
- [ ] Event type strings unchanged
- [ ] Strict/lenient contract unchanged
- [ ] Error kind contract unchanged

## Reviewer Gate

- [ ] `scripts/ci/review-wave8a-step1.sh` exists
- [ ] Gate enforces allowlist-only + workflow-ban
- [ ] Gate executes fmt + clippy + targeted A2A tests
- [ ] Gate enforces no-increase drift counters
- [ ] Gate fails when `crates/assay-adapter-a2a/src/lib.rs` changes in Step1
