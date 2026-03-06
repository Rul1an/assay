# Wave C1 B2 Review Pack - commands/replay.rs Mechanical Split

## Intent

Mechanically split replay command internals into focused modules while preserving the `replay::run` contract.

## Scope

- `crates/assay-cli/src/cli/commands/replay.rs` (removed)
- `crates/assay-cli/src/cli/commands/replay/mod.rs`
- `crates/assay-cli/src/cli/commands/replay/flow.rs`
- `crates/assay-cli/src/cli/commands/replay/run_args.rs`
- `crates/assay-cli/src/cli/commands/replay/failure.rs`
- `crates/assay-cli/src/cli/commands/replay/manifest.rs`
- `crates/assay-cli/src/cli/commands/replay/fs_ops.rs`
- `crates/assay-cli/src/cli/commands/replay/provenance.rs`
- `crates/assay-cli/src/cli/commands/replay/tests.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave-c1-b2-replay.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c1-b2-replay.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b2-replay.md`
- `scripts/ci/review-wave-c1-b2-replay.sh`

## Non-goals

- No replay behavior change.
- No exit code or reason mapping change.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<c1-b1-commit> bash scripts/ci/review-wave-c1-b2-replay.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli
```

## Reviewer 60s Scan

1. Confirm `mod.rs` is a thin facade re-exporting `run`.
2. Confirm key replay helpers are single-source in expected modules.
3. Confirm allowlist/workflow-ban/drift gates are hard-fail.
4. Run reviewer script and confirm PASS.
