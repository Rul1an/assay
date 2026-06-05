# SPLIT REVIEW PACK - Wave53 Step2 - High-Readiness Hotspot Split

## Scope

Step2 mechanically splits the high-readiness Wave53 targets behind stable facades:

- `crates/assay-core/src/report/summary.rs`
- `crates/assay-cli/src/cli/commands/bundle.rs`
- `crates/assay-registry/src/lockfile.rs`

This PR should be reviewed as a stacked PR on `codex/wave53-hotspot-top2-9-step1`.

## Files

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step2.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step2.sh`
- `crates/assay-core/src/report/summary.rs`
- `crates/assay-core/src/report/summary/types.rs`
- `crates/assay-core/src/report/summary/metrics.rs`
- `crates/assay-core/src/report/summary/writer.rs`
- `crates/assay-cli/src/cli/commands/bundle.rs`
- `crates/assay-cli/src/cli/commands/bundle/implementation.rs`
- `crates/assay-cli/src/cli/commands/bundle/verify.rs`
- `crates/assay-cli/src/cli/commands/bundle/paths.rs`
- `crates/assay-cli/src/cli/commands/bundle/coverage.rs`
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-registry/src/lockfile_next/types.rs`

## Verification

Run the Step2 gate from the Step2 branch:

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step2.sh
```

The gate checks the stack diff against `codex/wave53-hotspot-top2-9-step1` by default. Use
`BASE_REF=<ref>` only when reviewing a differently named local stack base.

The gate runs:

```bash
cargo fmt --check
cargo check -p assay-registry
cargo test -q -p assay-registry lockfile
cargo check -p assay-core
cargo test -q -p assay-core --lib report::summary
cargo check -p assay-cli
cargo test -q -p assay-cli -- bundle
git diff --check
```

## Reviewer Focus

- Confirm the three facade files preserve public module paths and command entrypoints.
- Confirm moved code is mechanical and does not change summary JSON, bundle archive contents, or
  lockfile serialization.
- Confirm Step2 does not touch workflows, generated eBPF bindings, or later Wave53 targets.
- Confirm the plan update only reflects the actual Step2 module names.

## PR Timing

Open Step2 only after this gate passes locally. Step3 should wait until Step2 is green or merged,
because Step3 edits other CLI command facades and should not compound review drift while Step2 is
still under review.
