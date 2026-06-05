# Wave53 Step3 Checklist - CLI Command Split

- [ ] Step3 is stacked on `codex/wave53-hotspot-top2-9-step2`
- [ ] Plan reflects the actual Step3 module layout
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step3.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step3.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step3.sh`
- [ ] `crates/assay-cli/src/cli/commands/runner_spike.rs` remains a thin facade
- [ ] `crates/assay-cli/src/cli/commands/doctor.rs` remains a thin facade
- [ ] Runner-spike Clap types remain re-exported from the facade
- [ ] Doctor fix, preview, and parse-error behavior remains module-private behind the command facade
- [ ] No edits under `.github/workflows/**`
- [ ] No edits to generated `crates/assay-ebpf/src/vmlinux.rs`
- [ ] No Step3 source edits outside the CLI command target allowlist
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step3.sh` passes
