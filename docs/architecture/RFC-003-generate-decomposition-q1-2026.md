# RFC-003: Generate Command Decomposition Plan (Q1 2026)

- Status: Proposed
- Date: 2026-02-09
- Owner: DX/Core
- Scope: `crates/assay-cli/src/cli/commands/generate.rs`
- Related:
  - `docs/architecture/RFC-002-code-health-remediation-q1-2026.md` (E5)
  - `docs/architecture/CODE-ANALYSIS-REPORT.md` (#27)

## 1. Context

`generate.rs` is currently ~1166 lines and combines multiple concerns:

1. CLI argument model and validation
2. Policy/output DTOs and serialization
3. Trace ingestion (`read_events`) and aggregation
4. Profile-based classification logic
5. Policy diffing and reporting
6. Top-level command orchestration (`run`)
7. Unit tests for several subsystems

This makes behavior changes harder to review and increases accidental drift risk when touching one concern.

## 2. Current Shape (Verified)

Current file: `crates/assay-cli/src/cli/commands/generate.rs` (1166 lines).

High-level concern map:

- Args + validation: `GenerateArgs`, `validate`
- Model DTOs: `Policy`, `Meta`, `Section`, `NetSection`, `Entry`
- Single-run path: `read_events`, `aggregate`, `generate_from_trace`
- Profile path: `generate_from_profile`, `classify_entry`, `make_entry_profile`
- Diff path: `PolicyDiff` helpers, `diff_policies`, `print_policy_diff`
- Entry point: `run`
- Tests: classification + diff behavior

## 3. Constraints (Hard Stop-Lines)

1. No output contract changes:
   - No schema drift in generated policy format (yaml/json)
   - No CLI flag behavior changes
   - No change in exit code behavior (`run` still returns `Result<i32>`, with `Ok(0)` on success)
2. No semantic drift in classification:
   - Wilson/laplace/min_runs/new_is_risky logic remains identical
3. No diff-output semantic drift:
   - Added/removed/changed calculation remains identical
4. No hidden tolerance changes:
   - `read_events` parse/skip/error-rate behavior unchanged

## 4. Research Baseline (Best Practices, Feb 2026)

Primary guidance used for this plan:

1. Rust API Guidelines: keep modules focused and APIs explicit (`C-CONV`, `C-STRUCT`, `C-ITER`).
   - https://rust-lang.github.io/api-guidelines/checklist.html
2. Clippy docs: prefer specific maintainability lints and tests over abstract complexity scores.
   - https://rust-lang.github.io/rust-clippy/stable/index.html
3. Test execution discipline: deterministic, fast feedback loops (`nextest` and targeted gates).
   - https://www.nexte.st/

Applied policy for this RFC:

- Test-first characterization on behavior-sensitive paths
- Extract-only changes per phase
- Small, linear PRs with explicit non-goals

## 5. Target Module Layout

Proposed end-state under `crates/assay-cli/src/cli/commands/generate/`:

- `mod.rs`:
  - public `run(args: GenerateArgs) -> Result<i32>`
  - module wiring/re-exports
- `args.rs`:
  - `GenerateArgs`
  - `GenerateArgs::validate`
- `model.rs`:
  - `Policy`, `Meta`, `Section`, `NetSection`, `Entry`
  - `serialize`
- `ingest.rs`:
  - `Stats`, `Aggregated`
  - `read_events`, `aggregate`
- `profile.rs`:
  - `generate_from_trace`, `generate_from_profile`
  - `classify_entry`, `make_entry_profile`, `make_entry_simple`
- `diff.rs`:
  - `EntryFingerprint`, `EntryChange`, `SectionDiff`, `PolicyDiff`
  - `parse_existing_policy`, `diff_policies`, `print_policy_diff`
- `tests.rs` (or per-module `#[cfg(test)]`):
  - migrated existing tests with identical assertions

Compatibility requirement:

- Keep command dispatch path unchanged (`generate::run` remains callable from command router).

## 6. Execution Plan

### G1 - Freeze Tests (Behavior Characterization)

Add targeted characterization tests before moving logic:

1. `read_events` contract:
   - skip empty/comment lines
   - unparsable lines count/warn behavior
   - hard error when all lines invalid
2. `run` mode gating:
   - requires exactly one of `--input` or `--profile`
3. classification invariants:
   - stable allow/review/skip transitions
   - risk override precedence
   - min-runs gate behavior
4. diff invariants:
   - added/removed/changed semantics unchanged

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo check -p assay-cli`

### G2 - Extract DTO/Args Layer (No Logic Moves Yet)

Scope:

- Move args + model DTOs + `serialize` to `args.rs` / `model.rs`
- Keep function bodies unchanged

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo clippy -p assay-cli -- -D warnings`

### G3 - Extract Ingestion/Aggregation

Scope:

- Move `Stats`, `Aggregated`, `read_events`, `aggregate` into `ingest.rs`
- Preserve all warning/error strings

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo check -p assay-cli`

### G4 - Extract Profile/Classification Logic

Scope:

- Move profile generation/classification helpers into `profile.rs`
- Keep algorithm and ordering unchanged

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo clippy -p assay-cli -- -D warnings`

### G5 - Extract Diff Subsystem

Scope:

- Move diff structs/helpers into `diff.rs`
- Keep summary output format and counts unchanged

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo check -p assay-cli`

### G6 - Final Orchestration Cleanup

Scope:

- Reduce `mod.rs` to orchestration only
- Keep `run` signature and return semantics unchanged

Gate:

- `cargo test -p assay-cli generate -- --nocapture`
- `cargo test -p assay-cli --lib -- --nocapture`
- `cargo clippy -p assay-cli -- -D warnings`

## 7. PR Slicing Strategy

Recommended PR sequence:

1. PR-G1: tests-only freeze
2. PR-G2: args/model extraction
3. PR-G3: ingest extraction
4. PR-G4: profile extraction
5. PR-G5: diff extraction
6. PR-G6: final `mod.rs` cleanup

Each PR must include:

- In-scope section
- Non-goals section
- Contract gates section
- Output impact statement (`none`)

## 8. Risks and Mitigations

1. Risk: subtle classification drift during extraction
   - Mitigation: G1 characterization tests and unchanged helper signatures
2. Risk: diff noise or changed reporting semantics
   - Mitigation: freeze diff tests and keep summary formatting assertions
3. Risk: accidental CLI behavior drift
   - Mitigation: mode-gating tests and unchanged `run` entrypoint

## 9. Definition of Done

RFC-003 is done when:

1. `generate.rs` orchestration module is reduced and concerns are split into focused modules
2. Existing behavior is preserved under frozen tests
3. No output/schema/exit behavior changes are introduced
4. All G1-G6 gates are green on CI
