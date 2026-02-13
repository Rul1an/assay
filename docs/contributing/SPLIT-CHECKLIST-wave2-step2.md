# Wave 2 Step 2 checklist (mechanical runtime split)

Scope lock:
- Step 2 is a mechanical split only.
- Public entrypoints and signatures stay stable.
- No behavior/perf changes in Commit A/B/C.
- `demo/` untouched.

## Commit slicing

- Commit A: compile-safe scaffolds only (`runner_next/*`, `mandate_store_next/*`), not wired.
- Commit B: mechanical 1:1 function moves behind stable facades (`runner.rs`, `mandate_store.rs`).
- Commit C: review artifacts, boundary gates, reviewer script.

## Target layout

```text
crates/assay-core/src/engine/runner_next/
  mod.rs
  execute.rs
  retry.rs
  baseline.rs
  scoring.rs
  cache.rs
  errors.rs
  tests.rs

crates/assay-core/src/runtime/mandate_store_next/
  mod.rs
  schema.rs
  upsert.rs
  consume.rs
  revocation.rs
  stats.rs
  txn.rs
  tests.rs
```

## Boundary intent

`runner_next`
- `execute.rs`: orchestration only.
- `retry.rs`: retry/backoff classification only.
- `baseline.rs`: baseline compare helpers only.
- `scoring.rs`: score/enrichment mapping only.
- `cache.rs`: cache calls only.
- `errors.rs`: error constructors/mapping only.

`mandate_store_next`
- `schema.rs`: schema bootstrap/version checks only.
- `upsert.rs`: upsert transaction path.
- `consume.rs`: consume/idempotency path.
- `revocation.rs`: revocation path.
- `stats.rs`: counters/read-side stats.
- `txn.rs`: shared transaction wrappers.

## Commit A gates (copy/paste)

```bash
set -euo pipefail

# 1) New scaffolds exist.
test -f crates/assay-core/src/engine/runner_next/mod.rs
test -f crates/assay-core/src/runtime/mandate_store_next/mod.rs

# 2) Existing modules stay active for Commit A.
rg -n "pub mod runner;" crates/assay-core/src/engine/mod.rs
rg -n "mod mandate_store;" crates/assay-core/src/runtime/mod.rs

# 3) No wiring to *_next from active modules yet.
if rg -n "runner_next" crates/assay-core/src/engine/mod.rs crates/assay-core/src/engine/runner.rs; then
  echo "runner_next wired too early in Commit A"
  exit 1
fi
if rg -n "mandate_store_next" crates/assay-core/src/runtime/mod.rs crates/assay-core/src/runtime/mandate_store.rs; then
  echo "mandate_store_next wired too early in Commit A"
  exit 1
fi
```

## Step 2 baseline validation

```bash
# Existing Step 1 guardrails remain green
bash scripts/ci/review-wave2-step1.sh
```

## Definition of done

- Commit A/B/C each remain reviewable and rollback-friendly.
- Public API and contract tests remain stable through Step 2.
- Boundary gates pass with no cross-module leakage.
