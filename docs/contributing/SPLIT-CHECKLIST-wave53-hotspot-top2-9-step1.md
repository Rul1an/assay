# Wave53 Step1 Checklist - Top 2-9 Hotspot Freeze

- [ ] Plan exists: `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`
- [ ] Wave53 scope is fixed to the selected top 2-9 snapshot, not dynamic current LOC order
- [ ] `crates/assay-ebpf/src/vmlinux.rs` is explicitly out of scope
- [ ] Step1 has no Rust source edits in the Wave53 target files
- [ ] Step1 has no test edits
- [ ] Step1 has no workflow edits
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh` passes
