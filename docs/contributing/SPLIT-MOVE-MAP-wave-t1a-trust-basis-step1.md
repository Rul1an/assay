# Wave T1a Trust Basis Step1 Move Map

## New surfaces

- `crates/assay-evidence/src/trust_basis.rs`
  - canonical trust-basis schema
  - claim classification
  - canonical JSON serialization
  - unit tests for fixed claim set and deterministic regeneration
- `crates/assay-cli/src/cli/args/trust_basis.rs`
  - low-level CLI argument surface
- `crates/assay-cli/src/cli/commands/trust_basis.rs`
  - verified bundle -> `trust-basis.json` generation path
- `crates/assay-cli/tests/trust_basis_test.rs`
  - command-level artifact tests

## Existing surfaces touched

- `crates/assay-evidence/src/lib.rs`
  - module export + re-exports
- `crates/assay-cli/src/cli/args/mod.rs`
  - top-level `trust-basis` command registration
- `crates/assay-cli/src/cli/commands/mod.rs`
  - command module registration
- `crates/assay-cli/src/cli/commands/dispatch.rs`
  - command dispatch wiring

## Explicit non-moves

- no `trustcard.json` / `trustcard.md`
- no pack semantics changes
- no engine/policy changes
- no raw OTel ingest semantics changes
