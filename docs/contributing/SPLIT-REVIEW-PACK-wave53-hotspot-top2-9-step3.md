# SPLIT REVIEW PACK - Wave53 Step3 - CLI Command Split

## Scope

Step3 mechanically splits the medium-readiness CLI command hotspots behind stable facades:

- `crates/assay-cli/src/cli/commands/runner_spike.rs`
- `crates/assay-cli/src/cli/commands/doctor.rs`

This PR should be reviewed as a stacked PR on `codex/wave53-hotspot-top2-9-step2`.

## Files

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step3.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step3.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step3.sh`
- `crates/assay-cli/src/cli/commands/runner_spike.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/args.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/implementation.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/spec.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/phases.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/cgroup.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/logs.rs`
- `crates/assay-cli/src/cli/commands/runner_spike/exit_status.rs`
- `crates/assay-cli/src/cli/commands/doctor.rs`
- `crates/assay-cli/src/cli/commands/doctor/implementation.rs`
- `crates/assay-cli/src/cli/commands/doctor/fixes.rs`
- `crates/assay-cli/src/cli/commands/doctor/patching.rs`
- `crates/assay-cli/src/cli/commands/doctor/parse_error.rs`

## Verification

Run the Step3 gate from the Step3 branch:

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step3.sh
```

The gate checks the stack diff against `codex/wave53-hotspot-top2-9-step2` by default. Use
`BASE_REF=<ref>` only when reviewing a differently named local stack base.

The gate runs:

```bash
cargo fmt --check
cargo check -p assay-cli
cargo test -q -p assay-cli -- runner_spike
cargo test -q -p assay-cli -- doctor
cargo clippy -p assay-cli --all-targets -- -D warnings
git diff --check
```

## Reviewer Focus

- Confirm `runner_spike.rs` preserves the public Clap type surface and command entrypoint.
- Confirm `doctor.rs` preserves text/json output, fix gating, dry-run preview, and parse-error repair behavior.
- Confirm moved code is mechanical and does not change exit codes, stdout/stderr text, archive contents, or cgroup/kernel-capture semantics.
- Confirm Step3 does not touch workflows, generated eBPF bindings, or later Wave53 targets.

## PR Timing

Open Step3 only after this gate passes locally and Step2 is green. Step4 should wait until Step3 is
green or merged because Step4 touches runner/eBPF boundaries and should not stack on a failing CLI
split.
