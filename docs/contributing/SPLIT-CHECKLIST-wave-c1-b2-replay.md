# SPLIT CHECKLIST - Wave C1 B2 (commands/replay.rs Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-cli/src/cli/commands/replay.rs`.
- [ ] No `.github/workflows/*` changes.
- [ ] No replay command behavior, exit-code mapping, or provenance semantics change.

## File Layout

- [ ] `crates/assay-cli/src/cli/commands/replay/mod.rs` is a thin facade.
- [ ] Split modules exist under `crates/assay-cli/src/cli/commands/replay/`:
  - `flow.rs`
  - `run_args.rs`
  - `failure.rs`
  - `manifest.rs`
  - `fs_ops.rs`
  - `provenance.rs`
  - `tests.rs`

## Behavior Freeze

- [ ] Public replay command entrypoint remains `replay::run`.
- [ ] Missing-dependency handling remains mapped to `ReasonCode::EReplayMissingDependency`.
- [ ] Run/summary provenance annotation contract remains unchanged.
- [ ] Existing replay unit tests still pass unchanged in intent.

## Boundary / Single-Source

- [ ] Replay orchestration lives in `flow.rs` only.
- [ ] Failure/exit JSON writing lives in `failure.rs` only.
- [ ] Bundle path resolution and source-run-id extraction live in `manifest.rs` only.
- [ ] Seed override + workspace/file materialization live in `fs_ops.rs` only.
- [ ] Provenance annotation lives in `provenance.rs` only.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-c1-b2-replay.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-cli`
- [ ] Gate enforces no-increase drift counters on replay split surface.
- [ ] Gate enforces facade-thinness and single-source checks.
