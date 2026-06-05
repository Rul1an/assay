# SPLIT REVIEW PACK - Wave53 Step1 - Top 2-9 Hotspot Freeze

## Scope

Step1 freezes the Wave53 selected hotspot scope and adds the reviewer gate. It is docs plus script
only.

## Files

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step1.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step1.sh`

## Baseline Note

The active repo-root inventory currently puts additional files above `crates/assay-ebpf/src/main.rs`.
Wave53 intentionally keeps the earlier selected 2-9 snapshot instead of expanding scope mid-wave.
Evidence schema-generation files and `coverage.rs` are not part of this wave.

## Verification

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh
```

For a clean Step1 PR branch, run the same gate with an explicit base ref:

```bash
BASE_REF=origin/main bash scripts/ci/review-wave53-hotspot-top2-9-step1.sh
```

The default local mode is intentionally tolerant of unrelated dirty Rust files because this branch
already contains work outside Wave53. Explicit `BASE_REF` mode keeps the stricter full
`cargo fmt --check` path for clean review branches.

## Reviewer Focus

- Confirm the wave does not overlap Wave51 scope.
- Confirm generated `crates/assay-ebpf/src/vmlinux.rs` stays out of scope.
- Confirm Step1 contains no Rust source, test, or workflow edits.
- Confirm later steps preserve stable facades before moving implementation bodies.

## Follow-Up

Step2 should create the high-readiness mechanical split for:

- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-core/src/report/summary.rs`
- `crates/assay-cli/src/cli/commands/bundle.rs`
