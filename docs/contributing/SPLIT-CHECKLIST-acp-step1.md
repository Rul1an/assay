# ACP Step1 Checklist (Freeze)

Scope lock:
- docs + reviewer gate script only
- no workflow changes
- no edits under `crates/assay-adapter-acp/**`

## Required outputs

- `docs/contributing/SPLIT-PLAN-wave14-acp.md`
- `docs/contributing/SPLIT-CHECKLIST-acp-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step1.md`
- `scripts/ci/review-acp-step1.sh`

## Freeze requirements

- Step2 target layout documented
- Step4 promote flow documented
- no tracked changes in `crates/assay-adapter-acp/**`
- no tracked changes in `crates/assay-adapter-api/**`
- no tracked changes in `crates/assay-evidence/**`
- no untracked files in `crates/assay-adapter-acp/**`

## Gate requirements

- `cargo fmt --check`
- `cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings`
- `cargo test -p assay-adapter-acp`
- allowlist-only diff
- workflow-ban

## Definition of done

- reviewer script passes with `BASE_REF=origin/main`
- Step1 diff is limited to the four freeze files
