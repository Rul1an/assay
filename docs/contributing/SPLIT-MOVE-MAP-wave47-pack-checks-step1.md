# SPLIT-MOVE-MAP — Wave47 Step1 — `lint/packs/checks.rs`

## Goal

Freeze the pack-check execution contract before any mechanical split of
`crates/assay-evidence/src/lint/packs/checks.rs`.

## Current hotspot boundary

- `crates/assay-evidence/src/lint/packs/checks.rs`
  currently co-locates top-level check dispatch, event-scoped checks, JSON-path matching,
  conditional execution, manifest checks, and finding/fingerprint metadata helpers.
- `crates/assay-evidence/src/lint/packs/schema.rs`
  and `schema_next/*` are already shipped from Wave46 and are explicitly out of scope.

## Frozen behavior boundaries

- check execution semantics remain identical across all existing `CheckDefinition` variants
- `json_path_exists` runtime matching and `value_equals` strict JSON equality remain identical
- the single-path `json_path_exists.value_equals` invariant remains identical in the
  validation/execution chain
- `event_type_exists`, `event_field_present`, `conditional`, and
  `g3_authorization_context_present` scoped-event behavior remains identical
- unsupported-check handling by pack kind remains identical
- finding emission / severity / rule-id / explanation coupling remains identical
- built-in/open pack parity and pack-lint baseline semantics remain identical

## Intended Step2 ownership split

- `checks.rs`: thin facade, stable exports, `ENGINE_VERSION`, and top-level dispatch
- `checks_next/event.rs`: event count/pairs/field/type/G3 checks plus scoped/glob helpers
- `checks_next/json_path.rs`: `check_json_path_exists` and `value_pointer`
- `checks_next/conditional.rs`: conditional execution and required-path failure messaging
- `checks_next/manifest.rs`: manifest-field checks
- `checks_next/finding.rs`: `create_finding`, severity/fingerprint/metadata helpers, and `event_location`

## Explicitly deferred

- `crates/assay-evidence/src/lint/packs/schema.rs`
- `crates/assay-evidence/src/lint/packs/schema_next/**`
- new check definitions or pack-engine version changes
- built-in/open pack content changes
- dispatch redesign, caching, or optimization work
