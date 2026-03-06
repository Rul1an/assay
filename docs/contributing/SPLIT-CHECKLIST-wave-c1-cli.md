# SPLIT CHECKLIST - Wave C1 Step A (CLI Surface Freeze)

## Scope Lock

- [ ] Step A is docs + reviewer gate only.
- [ ] No `.github/workflows/*` changes.
- [ ] No production-code changes in:
  - `crates/assay-cli/src/cli/args/mod.rs`
  - `crates/assay-cli/src/cli/commands/replay.rs`
  - `crates/assay-cli/src/env_filter.rs`

## Inventory Baseline (current mainline snapshot)

- [ ] `crates/assay-cli/src/cli/args/mod.rs` (794 LOC)
- [ ] `crates/assay-cli/src/cli/commands/replay.rs` (734 LOC)
- [ ] `crates/assay-cli/src/env_filter.rs` (767 LOC)

## Inventory Minimum (frozen for C1)

### `cli/args/mod.rs`

- [ ] Top-level command surface frozen: `Cli`, `Command` enum and all `*Args` structs in this file.
- [ ] Subcommand -> args-struct mapping frozen (single source currently in `mod.rs`).
- [ ] Existing parser test anchor frozen in `cli/args/tests.rs` (`cli_debug_assert`, sim parse tests under feature gate).

### `cli/commands/replay.rs`

- [ ] Replay run flow (`pub async fn run`) frozen.
- [ ] Failure/exit mapping path frozen:
  - `write_missing_dependency`
  - `write_replay_failure`
  - `RunOutcome::from_reason` + `ReasonCode::exit_code_for`
- [ ] Bundle create/verify boundary noted for split safety:
  - `bundle create/verify` flows live in `cli/commands/bundle.rs`
  - replay run flow consumes verified bundle and executes run path.

### `env_filter.rs`

- [ ] Env mode and filter pipeline frozen (`EnvMode`, `EnvFilter`, `EnvFilterResult`).
- [ ] Pattern matching boundary frozen (`matches_any_pattern`, `matches_pattern`).
- [ ] Normalize/sanitize behavior frozen (strict PATH sanitization, safe-path override, banner formatting).
- [ ] Existing unit tests in module remain behavior anchors.

## Behavior Freeze

- [ ] No CLI behavior change.
- [ ] No replay reason/exit mapping change.
- [ ] No env filter policy drift.
- [ ] No new dependencies in Step A.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-c1-cli-a.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-cli`
- [ ] Gate fails if any of the three target files are modified in Step A.
- [ ] Gate enforces no-increase drift counters on the three target files.
