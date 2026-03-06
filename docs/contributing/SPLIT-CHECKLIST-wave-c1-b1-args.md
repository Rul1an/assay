# SPLIT CHECKLIST - Wave C1 B1 (args/mod.rs Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-cli/src/cli/args/mod.rs` into `args/*` modules.
- [ ] No `.github/workflows/*` changes.
- [ ] No command behavior or parse contract changes.

## File Layout

- [ ] `crates/assay-cli/src/cli/args/mod.rs` is a thin facade.
- [ ] New split modules exist and are wired from the facade:
  - `coverage.rs`
  - `evidence.rs`
  - `import.rs`
  - `mcp.rs`
  - `replay.rs`
  - `runtime.rs`
  - `sim.rs`

## Behavior Freeze

- [ ] `Cli`, `Command`, and `ToolArgs` stay in `mod.rs`.
- [ ] Existing subcommand -> args type mapping in `Command` is unchanged.
- [ ] Existing parser/test anchors in `crates/assay-cli/src/cli/args/tests.rs` remain valid.
- [ ] No new dependencies.

## Boundary / Single-Source

- [ ] `mod.rs` does not reintroduce moved args structs/enums.
- [ ] Moved symbols are single-source in their target modules.
- [ ] Facade remains thin (module wiring + top-level command surface only).

## Reviewer Gate

- [ ] `scripts/ci/review-wave-c1-b1-args.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-cli`
- [ ] Gate enforces no-increase drift counters for split surface.
- [ ] Gate enforces facade-thinness and single-source checks.
