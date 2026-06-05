# SPLIT MOVE MAP - Wave53 Step1 - Top 2-9 Hotspot Freeze

## Step1 Movement

Step1 moves no Rust code.

The only allowed Step1 changes are planning and review-gate artifacts:

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`

## Frozen Target Files

Wave53 freezes these selected hotspot files for later mechanical splits:

- `crates/assay-runner-core/src/kernel.rs`
- `crates/assay-cli/src/cli/commands/runner_spike.rs`
- `crates/assay-ebpf/src/main.rs`
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-cli/src/cli/commands/bundle.rs`
- `crates/assay-core/src/report/summary.rs`
- `crates/assay-cli/src/cli/commands/doctor.rs`

## Explicit Non-Movement

- No edits under `crates/**/src/**/*.rs` in Step1.
- No edits under `crates/**/tests/**` in Step1.
- No edits under `.github/workflows/**`.
- No edits to `crates/assay-ebpf/src/vmlinux.rs`.

## Later Mechanical Direction

Step2 starts with the high-readiness files: `lockfile.rs`, `summary.rs`, and `bundle.rs`.
Step3 handles CLI command splits for `runner_spike.rs` and `doctor.rs`.
Step4 handles runner/eBPF boundaries for `kernel.rs` and `assay-ebpf/src/main.rs`.
Step5 handles the high-risk policy facade in `mcp/policy/mod.rs`.
