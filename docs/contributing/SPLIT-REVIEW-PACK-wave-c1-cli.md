# Wave C1 Step A Review Pack - CLI Surface Freeze

## Intent

Freeze CLI split scope and lock reviewer gates before any mechanical modularization of args/replay/env-filter surfaces.

## Scope

- `docs/contributing/SPLIT-CHECKLIST-wave-c1-cli.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-cli.md`
- `scripts/ci/review-wave-c1-cli-a.sh`

## Non-goals

- No mechanical code movement yet.
- No behavior or API changes.
- No workflow changes.

## Frozen Split Targets

- `crates/assay-cli/src/cli/args/mod.rs`
- `crates/assay-cli/src/cli/commands/replay.rs`
- `crates/assay-cli/src/env_filter.rs`

## Planned Next Mechanical Slices (C1)

1. `args/mod.rs` -> facade + thematic `args/*` modules.
2. `commands/replay.rs` -> `replay/*` modules (run/verify/create/format/errors split boundary).
3. `env_filter.rs` split only if inventory confirms parse/match/render seams remain mechanical.

## Validation Command

```bash
BASE_REF=<previous-step-commit> bash scripts/ci/review-wave-c1-cli-a.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli
```

## Reviewer 60s Scan

1. Confirm only Step A docs/script changed.
2. Confirm allowlist + workflow-ban + no-target-file-edits gates are hard fail.
3. Confirm inventory covers args mapping, replay flow/mapping, and env-filter boundaries.
4. Run reviewer script and confirm PASS.
