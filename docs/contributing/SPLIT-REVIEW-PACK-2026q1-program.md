# Review Pack: Split Refactor Program (Q1 2026)

Date: 2026-02-17
Mainline reference: `51dd45d5`
Baseline snapshot reference (pre-program): `6ae1d340`

## Intent

Provide a single reviewer-ready checkpoint for the full split-plan execution through Wave7C Step3.

## Scope

- Verify wave closure state against merged PR history.
- Verify plan status text matches repository reality.
- Verify LOC outcomes for the original hotspot set plus Wave7 additions.
- Verify no open wave PRs remain.

## Commands used

```bash
# Open/merged PR state
cd "$(git rev-parse --show-toplevel)"
gh pr list --repo Rul1an/assay --state open --json number,title,headRefName,baseRefName,mergeStateStatus,url
# Optional filter to verify "no open Wave PRs" explicitly:
gh pr list --repo Rul1an/assay --state open --json number,title | jq '.[] | select(.title|test("wave"; "i"))'
gh pr list --repo Rul1an/assay --state merged --limit 200 --json number,title,mergedAt,baseRefName,url
gh pr view 377 --json number,state,mergedAt,baseRefName,headRefName,url

# Plan status scan
rg -n "Wave ?[0-9]|wave[0-9]|Step ?[0-9]|status|Done|Merged|Active|Open" docs/architecture/PLAN-split-refactor-2026q1.md

# LOC verification on main
wc -l \
  crates/assay-evidence/src/bundle/writer.rs \
  crates/assay-registry/src/verify.rs \
  crates/assay-core/src/explain.rs \
  crates/assay-core/src/runtime/mandate_store.rs \
  crates/assay-core/src/engine/runner.rs \
  crates/assay-core/src/providers/trace.rs \
  crates/assay-registry/src/lockfile.rs \
  crates/assay-registry/src/cache.rs \
  crates/assay-cli/src/cli/commands/monitor.rs \
  crates/assay-core/src/runtime/authorizer.rs \
  crates/assay-evidence/src/lint/packs/loader.rs \
  crates/assay-core/src/storage/store.rs \
  crates/assay-core/src/judge/mod.rs \
  crates/assay-evidence/src/json_strict/mod.rs

# Largest production Rust files (exclude generated/tests)
find crates -name '*.rs' -type f \
  ! -path '*/target/*' \
  ! -path '*/tests/*' \
  ! -name 'vmlinux.rs' \
  ! -name '*tests.rs' \
  ! -path '*/test/*' -print0 \
| xargs -0 wc -l | awk '$1!="total"' | sort -nr | head -n 30
```

## Findings

1. Wave closure status
- No open Wave PRs.
- Wave7C Step3 merged via PR #377.
- Wave6 Step4 landed via PR #359, #360, #362.

2. Plan status consistency
- `docs/architecture/PLAN-split-refactor-2026q1.md` had stale in-progress wording for Wave5/Wave6/Wave7C and top status.
- Plan has been updated to match merged state.

3. LOC outcomes
- All major split hotspots reduced substantially.
- No production Rust file in the measured largest-files set is above 800 LOC.

## LOC evidence summary

Baseline values below are from pre-program snapshot `6ae1d340`; current values are from `main` at `51dd45d5`.

| File | Baseline | Current | Delta |
|---|---:|---:|---:|
| `crates/assay-evidence/src/bundle/writer.rs` | 1442 | 379 | -73.7% |
| `crates/assay-registry/src/verify.rs` | 1065 | 123 | -88.5% |
| `crates/assay-core/src/explain.rs` | 1057 | 11 | -99.0% |
| `crates/assay-core/src/runtime/mandate_store.rs` | 1046 | 748 | -28.5% |
| `crates/assay-core/src/engine/runner.rs` | 1042 | 661 | -36.6% |
| `crates/assay-core/src/providers/trace.rs` | 881 | 488 | -44.6% |
| `crates/assay-registry/src/lockfile.rs` | 863 | 649 | -24.8% |
| `crates/assay-registry/src/cache.rs` | 844 | 592 | -29.9% |
| `crates/assay-cli/src/cli/commands/monitor.rs` | 833 | 175 | -79.0% |
| `crates/assay-core/src/runtime/authorizer.rs` | 794 | 201 | -74.7% |
| `crates/assay-evidence/src/lint/packs/loader.rs` | 793 | 106 | -86.6% |
| `crates/assay-core/src/storage/store.rs` | 774 | 658 | -15.0% |
| `crates/assay-core/src/judge/mod.rs` | 712 | 71 | -90.0% |
| `crates/assay-evidence/src/json_strict/mod.rs` | 759 | 81 | -89.3% |

## Merge readiness statement (program-level)

- Split program through Wave7C Step3: **complete on main**.
- Remaining open PRs are operational/maintenance (`#365`) and not split-plan blockers.
- Largest-file scan is best-effort: excludes `*/tests/*` and `*tests.rs`, but may include `#[cfg(test)]` blocks inside production files.
