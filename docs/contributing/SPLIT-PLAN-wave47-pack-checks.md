# Wave47 Plan — `lint/packs/checks.rs` Kernel Split

## Goal

Split `crates/assay-evidence/src/lint/packs/checks.rs` behind a stable facade so pack-check
execution can be reviewed in smaller, responsibility-based modules without changing runtime
semantics or pack-visible findings.

Current hotspot baseline on `origin/main @ b5f359fa`:
- `crates/assay-evidence/src/lint/packs/checks.rs`: `785` LOC before Step2, `283` LOC after Step2
- `crates/assay-evidence/src/lint/packs/schema.rs`: `245` LOC after Wave46
- `crates/assay-evidence/tests/pack_engine_conditional_test.rs`: execution contract companion
- `crates/assay-evidence/tests/mcp_signal_followup_pack.rs`: G3/runtime parity companion
- `crates/assay-evidence/tests/a2a_discovery_card_followup_pack.rs`: `value_equals` boolean and open/built-in parity companion
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`: conditional support companion

## Status

- Wave46 closed on `main` via `#965`.
- Wave47 Step1 shipped on `main` via `#966`.
- Wave47 Step2 shipped on `main` via `#967`.
- Step3 is the closure slice for `checks.rs`.
- `schema.rs` and `schema_next/*` are already shipped from Wave46 and are explicitly out of scope.
- `checks.rs` follows `schema.rs` in the required `schema.rs -> checks.rs` order for `R4`.

## Frozen public surface

Wave47 freezes the expectation that Step2 keeps these check-layer items stable in meaning:

- `CheckContext`
- `CheckResult`
- `ENGINE_VERSION`
- `execute_check`

Step2 may reorganize internal ownership behind `checks.rs`, but must not redefine:

- check execution semantics for `event_count`, `event_pairs`, `event_field_present`,
  `event_type_exists`, `manifest_field`, `json_path_exists`,
  `g3_authorization_context_present`, and `conditional`
- `json_path_exists` and `value_equals` runtime behavior
- the single-path invariant for `json_path_exists.value_equals` in the validation/execution chain
- `event_type_exists`, `event_field_present`, `conditional`, and
  `g3_authorization_context_present` scoped-event behavior
- unsupported-check handling by pack kind
- finding emission semantics (canonical rule id, severity, message meaning, fingerprint/pack metadata coupling)
- built-in/open pack runtime parity for the existing follow-up pack line
- pack-lint baseline semantics for existing golden/baseline cases

## Step2 principles

- `checks.rs` stays the stable facade entrypoint.
- Step2 is mechanical relocation only.
- No check-dispatch drift.
- No pass/fail or finding-count drift.
- No severity / rule-id / explanation coupling drift.
- No built-in/open parity drift.
- No pack-engine spec or version-line drift.

## Step2 layout under review

- `crates/assay-evidence/src/lint/packs/checks.rs` keeps the stable facade entrypoint, top-level dispatch,
  `CheckContext`, `CheckResult`, `ENGINE_VERSION`, and existing inline tests
- `crates/assay-evidence/src/lint/packs/checks_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/event.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/json_path.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/manifest.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/finding.rs`

## Intended Step2 ownership split

- `checks.rs`: thin facade, stable exports, `ENGINE_VERSION`, and top-level dispatch surface
- `checks_next/event.rs`: event-count/pairs/field/type/G3 checks plus scoped-event and glob helpers
- `checks_next/json_path.rs`: `json_path_exists` execution and `value_pointer`
- `checks_next/conditional.rs`: conditional execution and missing-required-path messaging
- `checks_next/manifest.rs`: manifest-field execution
- `checks_next/finding.rs`: finding creation, event locations, fingerprints, and metadata helpers

## Step3 constraints

- Step3 keeps `checks.rs` as the stable facade entrypoint.
- Step3 keeps `checks_next/*` as the shipped implementation ownership boundary.
- Step3 is docs/gates only.
- No new module cuts.
- No behavior cleanup beyond internal follow-up notes.
- No execution, finding, parity, or validation-chain drift is allowed in Step3.

## Reviewer notes

Primary failure modes:

- moving check execution semantics while claiming a mechanical split
- relaxing `json_path_exists` / `value_equals` matching semantics
- letting the single-path `value_equals` invariant drift indirectly during a `checks.rs` split
- changing finding wording, severity, canonical IDs, or pack metadata coupling
- mixing new check kinds, engine bumps, or pack-content churn into the split

## Non-goals

- No edits to `crates/assay-evidence/src/lint/packs/schema.rs` or `schema_next/*`.
- No edits to `crates/assay-evidence/tests/**`.
- No edits under `packs/open/**`.
- No new check types, engine bump, spec expansion, or dispatch redesign.
- No finding-wording cleanup or test reorganization in this wave.
