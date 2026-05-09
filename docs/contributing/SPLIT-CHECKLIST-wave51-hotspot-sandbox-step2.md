# SPLIT CHECKLIST - Wave 51 Sandbox Step2

## Scope Lock

- Split `assay sandbox` implementation helpers behind the existing `pub async fn run(args)` entrypoint.
- Preserve CLI output, exit codes, env filtering, Landlock compatibility checks, scoped temp behavior, profile/evidence writes, dry-run violation handling, and degradation evidence semantics.
- Do not change policy semantics.
- Do not touch workflows or generated files.

## Files

- `crates/assay-cli/src/cli/commands/sandbox.rs`
- `crates/assay-cli/src/cli/commands/sandbox/child.rs`
- `crates/assay-cli/src/cli/commands/sandbox/degradation.rs`
- `crates/assay-cli/src/cli/commands/sandbox/env.rs`
- `crates/assay-cli/src/cli/commands/sandbox/profile.rs`
- `crates/assay-cli/src/cli/commands/sandbox/tmp.rs`
- `docs/contributing/SPLIT-CHECKLIST-wave51-hotspot-sandbox-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave51-hotspot-sandbox-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave51-hotspot-sandbox-step2.md`
- `scripts/ci/review-wave51-hotspot-sandbox-step2.sh`

## Drift Gates

- `sandbox.rs` non-test code stays under 250 lines.
- `tokio::process::Command` lives in `sandbox/child.rs`.
- scoped temp creation lives in `sandbox/tmp.rs`.
- `EnvFilter` construction lives in `sandbox/env.rs`.
- degradation payload construction lives in `sandbox/degradation.rs`.
- profile finish and deterministic evidence profile run id live in `sandbox/profile.rs`.

## Validation

```bash
cargo fmt --check
cargo check -p assay-cli
cargo clippy -p assay-cli --all-targets -- -D warnings
cargo test -p assay-cli sandbox
cargo test -p assay-cli --test profile_integration_test
bash scripts/ci/review-wave51-hotspot-sandbox-step2.sh
```

## Definition of Done

- Step 2 reviewer script passes.
- Sandbox unit and profile integration tests pass.
- LOC delta is reported.
- Next hotspot remains untouched.
