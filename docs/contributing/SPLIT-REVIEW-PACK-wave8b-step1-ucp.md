# Wave8B Step1 Review Pack - UCP Freeze

## Intent

Freeze UCP adapter behavior and lock reviewer gates before the Wave8B mechanical split.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave8b-step1-ucp.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave8b-step1-ucp.md`
- `scripts/ci/review-wave8b-step1.sh`

## Non-goals

- No mechanical code movement yet
- No API or behavior changes
- No workflow changes

## Step2 planned split map (frozen)

`crates/assay-adapter-ucp/src/lib.rs` ->

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
BASE_REF=origin/main bash scripts/ci/review-wave8b-step1.sh
```
