# Wave8A Step1 Review Pack - A2A Freeze

## Intent

Freeze A2A adapter behavior and lock reviewer gates before the Wave8A mechanical split.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave8a-step1-a2a.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8a-step1-a2a.md`
- `scripts/ci/review-wave8a-step1.sh`

## Non-goals

- No mechanical code movement yet
- No API or behavior changes
- No workflow changes

## Step2 planned split map (frozen)

`crates/assay-adapter-a2a/src/lib.rs` ->

- `adapter_impl/convert.rs`
- `adapter_impl/parse.rs`
- `adapter_impl/version.rs`
- `adapter_impl/fields.rs`
- `adapter_impl/mapping.rs`
- `adapter_impl/payload.rs`
- `adapter_impl/tests.rs`
- thin facade remains in `lib.rs`

## Validation Command

```bash
BASE_REF=origin/main bash scripts/ci/review-wave8a-step1.sh
```

This gate runs:

```bash
cargo fmt --check
cargo clippy -p assay-adapter-a2a -p assay-adapter-api --all-targets -- -D warnings
cargo test -p assay-adapter-a2a
bash scripts/ci/test-adapter-a2a.sh
```

## Reviewer 60s Scan

1. Verify only Step1 docs/script changed.
2. Verify gate has allowlist + workflow-ban.
3. Verify gate blocks production edits for A2A hotspot in Step1.
4. Run reviewer script and confirm PASS.
