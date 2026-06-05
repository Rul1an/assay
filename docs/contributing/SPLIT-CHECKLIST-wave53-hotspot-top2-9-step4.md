# Wave53 Step4 Checklist - Runner and eBPF Split

- [ ] Step4 is stacked on `codex/wave53-hotspot-top2-9-step3`
- [ ] Step3 PR checks are green before opening the Step4 PR
- [ ] Plan reflects the actual Step4 module layout
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step4.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step4.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step4.sh`
- [ ] `crates/assay-runner-core/src/kernel.rs` remains the public kernel facade
- [ ] Kernel decode, stats, health, note, and test helpers live under `kernel/*`
- [ ] `crates/assay-ebpf/src/main.rs` preserves tracepoint entrypoints and eBPF map declarations
- [ ] eBPF helper logic lives in `open_events.rs`, `connect_events.rs`, `fork_events.rs`, and `path_filter.rs`
- [ ] No edits under `.github/workflows/**`
- [ ] No edits to generated `crates/assay-ebpf/src/vmlinux.rs`
- [ ] No Step4 source edits outside the runner/eBPF allowlist
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step4.sh` passes
