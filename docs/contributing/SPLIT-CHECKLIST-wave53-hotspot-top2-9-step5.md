# Wave53 Step5 Checklist - Policy Facade Closure

- [ ] Step5 is stacked on `codex/wave53-hotspot-top2-9-step4`
- [ ] Step4 PR checks are green before opening the Step5 PR
- [ ] Plan reflects the actual Step5 module layout
- [ ] Move-map exists: `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step5.md`
- [ ] Review pack exists: `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step5.md`
- [ ] Review script exists: `scripts/ci/review-wave53-hotspot-top2-9-step5.sh`
- [ ] `crates/assay-core/src/mcp/policy/mod.rs` remains the public policy facade
- [ ] Policy public types live in `policy/types.rs`
- [ ] Legacy constraints deserialization compatibility lives in `policy/deserialize.rs`
- [ ] Tool pattern matching lives in `policy/matcher.rs`
- [ ] Typed policy decision contracts and tests live in `policy/contracts.rs`
- [ ] Existing `policy/engine_next/*` files are untouched
- [ ] No edits under `.github/workflows/**`
- [ ] No source edits outside the Step5 allowlist
- [ ] `bash scripts/ci/review-wave53-hotspot-top2-9-step5.sh` passes
