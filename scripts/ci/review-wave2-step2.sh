#!/usr/bin/env bash
set -euo pipefail

base_ref="${1:-origin/codex/wave2-step1-behavior-freeze}"
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
  local matches leaked
  matches="$("$rg_bin" -n "$pattern" "$root" -g'*.rs' || true)"
  if [ -z "$matches" ]; then
    echo "expected at least one match for: $pattern"
    exit 1
  fi
  leaked="$(echo "$matches" | "$rg_bin" -v "$allowed" || true)"
  if [ -n "$leaked" ]; then
    echo "forbidden match outside allowed file:"
    echo "$leaked"
    exit 1
  fi
}

strip_code_only() {
  local file="$1"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "$file"
}

echo "== Wave2 Step2 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Wave2 Step2 contract tests =="
cargo test -p assay-core --lib runner_contract_results_sorted_by_test_id -- --nocapture
cargo test -p assay-core --lib runner_contract_progress_sink_reports_done_total -- --nocapture
cargo test -p assay-core --lib runner_contract_relative_baseline_missing_warns_in_helper -- --nocapture
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Wave2 Step2 boundary gates =="
check_only_file_matches "attempts\\.push\\(" \
  crates/assay-core/src/engine/runner_next \
  "runner_next/retry.rs"

check_only_file_matches "BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b|transaction\\(|\\bTransaction\\b" \
  crates/assay-core/src/runtime/mandate_store_next \
  "mandate_store_next/txn.rs"

strip_code_only crates/assay-core/src/engine/runner.rs > /tmp/wave2_runner_code_only.rs
strip_code_only crates/assay-core/src/runtime/mandate_store.rs > /tmp/wave2_mandate_code_only.rs

check_no_match "JoinSet|Semaphore|tokio::spawn|BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b" \
  /tmp/wave2_runner_code_only.rs

check_no_match "INSERT INTO|UPDATE\\s+\\w+\\s+SET|SELECT\\s+.+\\s+FROM|BEGIN IMMEDIATE|\\bCOMMIT\\b|\\bROLLBACK\\b" \
  /tmp/wave2_mandate_code_only.rs

echo "== Wave2 Step2 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      "^crates/assay-core/src/engine/runner.rs$|^crates/assay-core/src/engine/runner_next/|^crates/assay-core/src/runtime/mandate_store.rs$|^crates/assay-core/src/runtime/mandate_store_next/|^docs/contributing/SPLIT-CHECKLIST-wave2-step2.md$|^docs/contributing/SPLIT-MOVE-MAP-wave2-step2.md$|^scripts/ci/review-wave2-step2.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$" || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave2 Step2 reviewer script: PASS"
