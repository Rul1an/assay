# SPLIT CHECKLIST - ADR-015 BYOS Phase 1 Step1

## Scope discipline
- [ ] Only these files changed:
  - `docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md`
  - `docs/contributing/SPLIT-CHECKLIST-adr015-byos-phase1-step1.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-adr015-byos-phase1-step1.md`
  - `scripts/ci/review-adr015-phase1-step1.sh`
- [ ] No `.github/workflows/*` changes
- [ ] No `crates/` changes
- [ ] No runtime code changes

## Contract freeze
- [ ] `store-status` command contract is explicit:
  - args: `--store`, `--store-config`, `--format`
  - precedence: `--store` > `ASSAY_STORE_URL` > `--store-config` > default lookup
  - JSON output shape is frozen
  - exit codes are frozen (0/1/2)
  - checks: reachable, readable, writable, bundle_count, object_lock (best-effort)
- [ ] Config design decision is explicit:
  - option A: separate file (`.assay/store.yaml`)
  - `EvalConfig` is untouched
  - config shape is frozen (url, region, allow_http, path_style)
  - credentials stay in env vars only
- [ ] Provider docs scope is explicit:
  - Phase 1: AWS S3, Backblaze B2, MinIO
  - Wasabi, R2, others deferred
- [ ] Out-of-scope items are explicit

## Wave structure
- [ ] Step 1/2/3 deliverables are explicit
- [ ] Step 2 frozen paths are explicit (`assay-core/config.rs`, `model/types.rs`, workflows)
- [ ] Step 3 is docs+gate-only

## Validation
- [ ] `BASE_REF=origin/main bash scripts/ci/review-adr015-phase1-step1.sh` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo clippy -p assay-evidence --all-targets -- -D warnings` passes
- [ ] Existing evidence tests pass: `cargo test -p assay-evidence`
