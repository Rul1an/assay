# Review Pack: Wave4 Step2 (lockfile/cache mechanical split)

Intent:
- Mechanical split of lockfile/cache implementation behind stable facades.
- Preserve behavior and public symbol paths.

Scope:
- `crates/assay-registry/src/lockfile.rs`
- `crates/assay-registry/src/cache.rs`
- `crates/assay-registry/src/lockfile_next/*`
- `crates/assay-registry/src/cache_next/*`
- `docs/contributing/SPLIT-MOVE-MAP-wave4-step2.md`
- `docs/contributing/SPLIT-CHECKLIST-wave4-step2.md`
- `scripts/ci/review-wave4-step2.sh`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Verification command:
```bash
BASE_REF=origin/codex/wave3-step1-behavior-freeze-v2 bash scripts/ci/review-wave4-step2.sh
```

Executed and passing:
- `cargo fmt --check`
- `cargo clippy -p assay-registry --all-targets -- -D warnings`
- `cargo check -p assay-registry`
- Lockfile/cache anchor subset tests
- Delegation gates (facade -> `*_next` impl paths)
- Single-source gates:
  - ordering path only in `lockfile_next/format.rs`
  - atomic write/rename path only in `cache_next/io.rs`
- Step2 diff allowlist

No behavior/perf changes intended:
- no output/error-string rewrites
- no dependency/Cargo changes
- no demo/ changes
