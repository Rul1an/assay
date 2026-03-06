# SPLIT CHECKLIST - Wave C1 C (Closure)

## Scope Lock

- [ ] Closure is docs + closure gate only.
- [ ] No `.github/workflows/*` changes.
- [ ] No production code edits in closure step.

## Final Layout Validation

- [ ] CLI args surface finalized:
  - `crates/assay-cli/src/cli/args/mod.rs` (facade)
  - thematic modules for args categories (`coverage`, `evidence`, `import`, `mcp`, `replay`, `runtime`, `sim`)
- [ ] Replay command surface finalized:
  - `crates/assay-cli/src/cli/commands/replay/mod.rs` (facade)
  - `flow`, `run_args`, `failure`, `manifest`, `fs_ops`, `provenance`, `tests`
- [ ] Env filter surface finalized:
  - `crates/assay-cli/src/env_filter/mod.rs` (facade)
  - `engine`, `matcher`, `patterns`, `tests`

## Reviewer Gate

- [ ] `scripts/ci/review-wave-c1-c-closure.sh` exists.
- [ ] Gate enforces allowlist-only + workflow-ban.
- [ ] Gate executes:
  - `cargo fmt --check`
  - `cargo clippy -p assay-cli --all-targets -- -D warnings`
  - `cargo test -p assay-cli`
- [ ] Gate asserts old monolith files are absent and key facades remain thin.
