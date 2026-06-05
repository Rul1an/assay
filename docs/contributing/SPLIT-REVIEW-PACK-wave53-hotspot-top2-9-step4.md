# SPLIT REVIEW PACK - Wave53 Step4 - Runner and eBPF Split

## Scope

Step4 mechanically splits the runner kernel-layer and eBPF monitor hotspots behind stable facades:

- `crates/assay-runner-core/src/kernel.rs`
- `crates/assay-ebpf/src/main.rs`

This PR should be reviewed as a stacked PR on `codex/wave53-hotspot-top2-9-step3`.

## Files

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step4.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step4.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step4.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step4.sh`
- `crates/assay-runner-core/src/kernel.rs`
- `crates/assay-runner-core/src/kernel/decode.rs`
- `crates/assay-runner-core/src/kernel/stats.rs`
- `crates/assay-runner-core/src/kernel/health.rs`
- `crates/assay-runner-core/src/kernel/notes.rs`
- `crates/assay-runner-core/src/kernel/tests.rs`
- `crates/assay-ebpf/src/main.rs`
- `crates/assay-ebpf/src/open_events.rs`
- `crates/assay-ebpf/src/connect_events.rs`
- `crates/assay-ebpf/src/fork_events.rs`
- `crates/assay-ebpf/src/path_filter.rs`

## Verification

Run the Step4 gate from the Step4 branch:

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step4.sh
```

The gate checks the stack diff against `codex/wave53-hotspot-top2-9-step3` by default. Use
`BASE_REF=<ref>` only when reviewing a differently named local stack base.

The gate runs:

```bash
cargo fmt --check
cargo check -p assay-runner-core
cargo test -q -p assay-runner-core
cargo clippy -p assay-runner-core --all-targets -- -D warnings
cargo check -p assay-ebpf
git diff --check
```

## Reviewer Focus

- Confirm `kernel.rs` preserves the public kernel-layer API and archive application behavior.
- Confirm decode, stats, health downgrade, note formatting, and network coverage labels are moved
  mechanically.
- Confirm eBPF tracepoint entrypoints and map declarations remain in `main.rs`.
- Confirm moved eBPF helpers do not change tracepoint names, map names, event payloads, loader path
  filtering, dedup semantics, or fork/connect/open emission behavior.
- Confirm Step4 does not touch workflows, generated `vmlinux.rs`, or the Step5 policy target.

## PR Timing

Open Step4 only after this gate passes locally and Step3 is green. Step5 should wait until Step4 is
green or merged because Step5 is the final policy closure and should not stack on a failing
runner/eBPF split.
