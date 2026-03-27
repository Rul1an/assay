# SPLIT-MOVE-MAP — Wave47 Step3 — `lint/packs/checks.rs` Closure

## Shipped layout

Wave47 Step2 is now the shipped split shape on `main`:
- `crates/assay-evidence/src/lint/packs/checks.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/mod.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/event.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/json_path.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/conditional.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/manifest.rs`
- `crates/assay-evidence/src/lint/packs/checks_next/finding.rs`

## Ownership freeze

- `crates/assay-evidence/src/lint/packs/checks.rs`
  remains the stable facade for `CheckContext`, `CheckResult`, `ENGINE_VERSION`,
  top-level dispatch, unsupported-check handling, and inline contract tests.
- `crates/assay-evidence/src/lint/packs/checks_next/event.rs`
  remains the event-family and G3/scoped-event helper boundary.
- `crates/assay-evidence/src/lint/packs/checks_next/json_path.rs`
  remains the `json_path_exists` and `value_pointer` boundary.
- `crates/assay-evidence/src/lint/packs/checks_next/conditional.rs`
  remains the conditional execution boundary.
- `crates/assay-evidence/src/lint/packs/checks_next/manifest.rs`
  remains the manifest-field execution boundary.
- `crates/assay-evidence/src/lint/packs/checks_next/finding.rs`
  remains the finding creation, fingerprint, location, and metadata helper boundary.

## Allowed follow-up after closure

- documentation updates only
- reviewer-gate tightening only
- internal visibility tightening only if it requires no code edits in this wave

## Explicitly deferred

- new module cuts
- new check types or engine bump
- dispatch redesign
- finding wording cleanup
- runtime execution cleanup
- built-in/open parity changes
- validation-chain or error-meaning changes
