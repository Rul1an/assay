# SPLIT CHECKLIST - Wave C1 B3 (env_filter.rs Mechanical Split)

## Scope Lock

- [ ] Mechanical move only for `crates/assay-cli/src/env_filter.rs`.
- [ ] No `.github/workflows/*` changes.
- [ ] No env filtering policy or banner behavior changes.

## File Layout

- [ ] `crates/assay-cli/src/env_filter/mod.rs` is a thin facade.
- [ ] Split modules exist under `crates/assay-cli/src/env_filter/`:
  - `engine.rs`
  - `matcher.rs`
  - `patterns.rs`
  - `tests.rs`

## Behavior Freeze

- [ ] `EnvMode`, `EnvFilter`, `EnvFilterResult` semantics unchanged.
- [ ] `matches_any_pattern` matching semantics unchanged.
- [ ] Scrub/strict/passthrough behavior unchanged.
- [ ] Existing unit tests remain behavior anchors.

## Boundary / Single-Source

- [ ] core filter logic is single-source in `engine.rs`.
- [ ] pattern constants are single-source in `patterns.rs`.
- [ ] glob matching logic is single-source in `matcher.rs`.
- [ ] facade only wires/re-exports symbols.

## Reviewer Gate

- [ ] `scripts/ci/review-wave-c1-b3-env-filter.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-cli`
- [ ] Gate enforces no-increase drift counters and single-source checks.
