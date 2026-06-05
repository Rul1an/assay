# Wave53 Step2 Checklist - High-Readiness Hotspot Split

- [ ] Step2 is stacked on `codex/wave53-hotspot-top2-9-step1`
- [ ] Plan reflects the actual Step2 module layout
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step2.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step2.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step2.sh`
- [ ] `crates/assay-core/src/report/summary.rs` remains a thin facade
- [ ] `crates/assay-cli/src/cli/commands/bundle.rs` remains a thin command facade
- [ ] `crates/assay-registry/src/lockfile.rs` keeps public lockfile types re-exported
- [ ] No edits under `.github/workflows/**`
- [ ] No edits to generated `crates/assay-ebpf/src/vmlinux.rs`
- [ ] No Step2 source edits outside the high-readiness target allowlist
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step2.sh` passes
