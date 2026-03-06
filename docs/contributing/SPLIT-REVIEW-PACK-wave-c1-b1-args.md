# Wave C1 B1 Review Pack - args/mod.rs Mechanical Split

## Intent

Mechanically split `cli/args/mod.rs` into thematic `args/*` modules behind a stable, thin facade.

## Scope

- `crates/assay-cli/src/cli/args/mod.rs`
- `crates/assay-cli/src/cli/args/coverage.rs`
- `crates/assay-cli/src/cli/args/evidence.rs`
- `crates/assay-cli/src/cli/args/import.rs`
- `crates/assay-cli/src/cli/args/mcp.rs`
- `crates/assay-cli/src/cli/args/replay.rs`
- `crates/assay-cli/src/cli/args/runtime.rs`
- `crates/assay-cli/src/cli/args/sim.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave-c1-b1-args.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c1-b1-args.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-b1-args.md`
- `scripts/ci/review-wave-c1-b1-args.sh`

## Non-goals

- No CLI behavior changes.
- No replay contract changes.
- No workflow changes.

## Validation Command

```bash
BASE_REF=<c1-a-commit> bash scripts/ci/review-wave-c1-b1-args.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli
```

## Reviewer 60s Scan

1. Confirm `mod.rs` is thin facade + command surface.
2. Confirm moved symbols are single-source in split modules.
3. Confirm no scope leaks (allowlist-only, workflow-ban).
4. Run reviewer script and confirm PASS.
