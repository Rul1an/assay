# Wave46 Plan — `lint/packs/schema.rs` Kernel Split

## Goal

Split `crates/assay-evidence/src/lint/packs/schema.rs` behind a stable facade so the pack-schema contract can be reviewed in smaller, responsibility-based modules without changing validation behavior.

Current hotspot baseline on `origin/main @ dcac6383`:
- `crates/assay-evidence/src/lint/packs/schema.rs`: `844` LOC
- `crates/assay-evidence/src/lint/packs/checks.rs`: `785` LOC
- `crates/assay-evidence/tests/pack_engine_conditional_test.rs`: conditional contract companion
- `crates/assay-evidence/tests/a2a_discovery_card_followup_pack.rs`: built-in/open parity companion
- `crates/assay-evidence/tests/mcp_signal_followup_pack.rs`: built-in/open parity companion

## Step sequence

1. Step1 freeze/gates only — merged on `main` via `#963`
2. Step2 mechanical split to `schema_next/*`
3. Step3 closure / micro-cleanup gates only

## Frozen public surface

Wave46 freezes the expectation that Step2 keeps these schema-layer types and contracts stable in
meaning:
- `PackDefinition`
- `PackRequirements`
- `PackRule`
- `CheckDefinition`
- `PackValidationError`
- `SupportedConditionalCheck`
- `SupportedConditionalClause`

Step2 may reorganize internal ownership behind `schema.rs`, but must not redefine:
- pack YAML parse/validation behavior
- `json_path_exists` and `value_equals` validation rules
- the single-path requirement for `value_equals`
- conditional-shape acceptance vs unsupported classification
- loader-visible built-in/open pack loadability and parity assumptions
- validation error category or reason-string meaning

## Step2 scope

Allowed implementation files:
- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/types.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/serde.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/validation.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/errors.rs`

Allowed review artifacts:
- `docs/contributing/SPLIT-PLAN-wave46-pack-schema.md`
- `docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step2.md`
- `scripts/ci/review-wave46-pack-schema-step2.sh`

Step2 principles:
- 1:1 body moves
- stable schema types and validation behavior
- no `json_path_exists` / `value_equals` drift
- no conditional-shape or unsupported-path drift
- no loader or built-in/open pack parity drift
- no edits under `crates/assay-evidence/tests/**`
- no edits under `packs/open/**`
- no workflow edits

Target layout:
- `schema.rs` as thin facade + stable exports + existing inline tests
- `schema_next/types.rs`
- `schema_next/serde.rs`
- `schema_next/validation.rs`
- `schema_next/conditional.rs`
- `schema_next/errors.rs`

## Step3 (closure)

Docs+gate-only closure slice that re-runs Step2 invariants and limits any follow-up to
micro-cleanup only.

## Reviewer notes

Primary failure modes:
- moving validation semantics while claiming a mechanical split
- relaxing `json_path_exists.value_equals` or conditional-shape rules during decomposition
- changing loader-visible pack acceptance behavior via schema-only churn
- mixing `checks.rs` execution logic into the `schema.rs` wave

## Non-goals

- No edits to `crates/assay-evidence/src/lint/packs/checks.rs`.
- No edits to `crates/assay-evidence/tests/**`.
- No changes under `packs/open/**` or built-in pack payloads.
- No error-message cleanup, schema redesign, or check DSL expansion.
