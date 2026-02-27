# Review Pack - ADR-026 Step3 (closure)

## Intent
Close ADR-026 initial rollout loop by adding closure review artifacts for the adapter API freeze and ACP MVP implementation.

## Scope
- docs/contributing/SPLIT-CHECKLIST-adr026-step3.md
- docs/contributing/SPLIT-REVIEW-PACK-adr026-step3.md
- scripts/ci/review-adr026-step3.sh

## Non-goals
- No workflow changes
- No new runtime behavior
- No ACP CLI wiring
- No A2A implementation yet

## What should already exist
- `crates/assay-adapter-api/`
- `crates/assay-adapter-acp/`
- `scripts/ci/test-adapter-acp.sh`
- ACP happy and negative fixtures under `scripts/ci/fixtures/adr026/acp/v2.11.0/`

## Verification
- `BASE_REF=origin/codex/adr026-step2-acp-mvp bash scripts/ci/review-adr026-step3.sh`
- `bash scripts/ci/test-adapter-acp.sh`
- `cargo fmt --check`
- `cargo clippy -p assay-adapter-acp -p assay-adapter-api -p assay-cli -- -D warnings`

## Reviewer 60s scan
1. Confirm Step3 changes are docs + gate only.
2. Confirm no `.github/workflows/*` changes.
3. Run the Step3 reviewer gate.
4. Confirm the checklist correctly describes Step1/Step2 deliverables.
