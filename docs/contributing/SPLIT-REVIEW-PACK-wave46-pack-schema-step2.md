# Wave46 Step2 Review Pack

## What changed

Mechanical split of `crates/assay-evidence/src/lint/packs/schema.rs` into `schema_next/*` behind a stable facade.

## What did not change

- No `checks.rs` edits
- No external test edits
- No pack payload edits
- No schema/check semantics changes

## Review focus

- `schema.rs` stays a thin export facade
- moved code reads as relocation, not redesign
- `value_equals` single-path rule remains unchanged
- conditional subset parsing remains unchanged
- validation error categories and meaning remain unchanged

## Required local gate

`BASE_REF=origin/main bash scripts/ci/review-wave46-pack-schema-step2.sh`
