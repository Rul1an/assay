# ACP Step1 Review Pack (Freeze)

## Intent

Freeze Wave14 scope for `assay-adapter-acp` split before any code movement.

## Scope

- `docs/contributing/SPLIT-PLAN-wave14-acp.md`
- `docs/contributing/SPLIT-CHECKLIST-acp-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step1.md`
- `scripts/ci/review-acp-step1.sh`

## Non-goals

- no edits in `crates/assay-adapter-acp/**`
- no edits in `crates/assay-adapter-api/**`
- no edits in `crates/assay-evidence/**`
- no behavior changes
- no workflow changes

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-acp-step1.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-acp
```

## Reviewer 60s scan

1. Confirm only Step1 docs/script changed.
2. Confirm no `assay-adapter-acp/**` tracked/untracked changes.
3. Confirm Step2/Step4 process is explicit.
4. Run reviewer script and expect PASS.
