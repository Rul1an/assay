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

## Commit B validation (copy/paste)

```bash
# Core compile/lint
bash scripts/ci/review-wave2-step1.sh

# Additional focused checks
cargo check -p assay-core
cargo test -p assay-core --lib runner_contract_results_sorted_by_test_id -- --nocapture
cargo test -p assay-core --lib runner_contract_progress_sink_reports_done_total -- --nocapture
cargo test -p assay-core --lib runner_contract_relative_baseline_missing_warns_in_helper -- --nocapture
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture
```

## Commit C boundary gates (copy/paste)

```bash
set -euo pipefail
rg_bin="$(command -v rg)"

check_no_match() {
  local pattern="$1"
  local path="$2"
  if "$rg_bin" -n "$pattern" "$path"; then
    echo "forbidden match in $path (pattern: $pattern)"
    exit 1
  fi
}

check_only_file_matches() {
  local pattern="$1"
  local root="$2"
  local allowed="$3"
  local matches
  matches="$("$rg_bin" -n "$pattern" "$root" -g'*.rs' || true)"
  if [ -z "$matches" ]; then
    echo "expected at least one match for: $pattern"
    exit 1
  fi
  local leaked
  leaked="$(echo "$matches" | "$rg_bin" -v "$allowed" || true)"
  if [ -n "$leaked" ]; then
    echo "forbidden match outside allowed file:"
    echo "$leaked"
    exit 1
  fi
}

strip_code_only() {
  local file="$1"
  awk '
    BEGIN {
      pending_cfg_test = 0
      skip_tests = 0
      depth = 0
    }
    {
      line = $0

      if (skip_tests) {
        opens = gsub(/\{/, "{", line)
        closes = gsub(/\}/, "}", line)
        depth += opens - closes
        if (depth <= 0) {
          skip_tests = 0
          depth = 0
        }
        next
      }

      if (pending_cfg_test) {
        if (line ~ /^[[:space:]]*#\[/ || line ~ /^[[:space:]]*$/) {
          next
        }
        if (line ~ /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*\{[[:space:]]*$/) {
          skip_tests = 1
          depth = 1
          pending_cfg_test = 0
          next
        }
        pending_cfg_test = 0
      }

      if (line ~ /^[[:space:]]*#\[cfg\(test\)\][[:space:]]*$/) {
        pending_cfg_test = 1
        next
      }

      print
    }
  ' "$file"
}

# Runner: attempt accounting is single-source in retry.rs
check_only_file_matches "attempts\\.push\\(" \
  crates/assay-core/src/engine/runner_next \
  "runner_next/retry.rs"

# Mandate store: transaction control is single-source in txn.rs
check_only_file_matches "BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b|transaction\\(|\\bTransaction\\b" \
  crates/assay-core/src/runtime/mandate_store_next \
  "mandate_store_next/txn.rs"

# Facade hygiene (code-only, excludes tests block)
strip_code_only crates/assay-core/src/engine/runner.rs | \
  check_no_match "JoinSet|Semaphore|tokio::spawn|BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b" /dev/stdin

strip_code_only crates/assay-core/src/runtime/mandate_store.rs | \
  check_no_match "INSERT INTO|UPDATE\\s+\\w+\\s+SET|SELECT\\s+.+\\s+FROM|BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b" /dev/stdin
```

## Scope diff allowlist

```bash
set -euo pipefail
base_ref="${1:-origin/codex/wave2-step1-behavior-freeze}"

git diff --name-only "${base_ref}...HEAD" | \
  rg -v \
    "^crates/assay-core/src/engine/runner.rs$|^crates/assay-core/src/engine/runner_next/|^crates/assay-core/src/runtime/mandate_store.rs$|^crates/assay-core/src/runtime/mandate_store_next/|^docs/contributing/SPLIT-CHECKLIST-wave2-step2.md$|^docs/contributing/SPLIT-MOVE-MAP-wave2-step2.md$|^scripts/ci/review-wave2-step2.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$"
# expect: empty output
```

## Reviewer script

```bash
bash scripts/ci/review-wave2-step1.sh
bash scripts/ci/review-wave2-step2.sh
```

## Definition of done

- Commit A/B/C each remain reviewable and rollback-friendly.
- Public API and contract tests remain stable through Step 2.
- Boundary gates pass with no cross-module leakage.
- Attempt accounting and transaction control each have one source of truth.
