#!/usr/bin/env bash
set -euo pipefail

base_ref="${1:-origin/main}"
rg_bin="$(command -v rg)"

count_in_ref() {
  local ref="$1"
  local file="$2"
  local pattern="$3"
  git show "${ref}:${file}" | awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local file="$1"
  local pattern="$2"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "$file" | "$rg_bin" -n "$pattern" || true
}

check_no_increase() {
  local file="$1"
  local pattern="$2"
  local label="$3"
  local before after
  before="$(count_in_ref "$base_ref" "$file" "$pattern" | wc -l | tr -d ' ')"
  after="$(count_in_worktree "$file" "$pattern" | wc -l | tr -d ' ')"
  echo "$label: before=$before after=$after"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: $label increased"
    exit 1
  fi
}

echo "== Wave2 Step1 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-core

echo "== Wave2 Step1 contract tests (runner) =="
cargo test -p assay-core --lib runner_contract_flake_fail_then_pass_classified_flaky -- --nocapture
cargo test -p assay-core --lib runner_contract_fail_after_retries_stays_fail -- --nocapture
cargo test -p assay-core --lib runner_contract_on_error_allow_marks_allowed_and_policy_applied -- --nocapture
cargo test -p assay-core --lib runner_contract_results_sorted_by_test_id -- --nocapture
cargo test -p assay-core --lib runner_contract_progress_sink_reports_done_total -- --nocapture
cargo test -p assay-core --lib runner_contract_relative_baseline_missing_warns_in_helper -- --nocapture

echo "== Wave2 Step1 contract tests (mandate store) =="
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture

echo "== Wave2 Step1 drift gates =="
check_no_increase "crates/assay-core/src/engine/runner.rs" "unwrap\\(|expect\\(" "runner unwrap/expect (code-only)"
check_no_increase "crates/assay-core/src/engine/runner.rs" "\\bunsafe\\b" "runner unsafe"
check_no_increase "crates/assay-core/src/engine/runner.rs" "println!|eprintln!" "runner stdout/stderr"
check_no_increase "crates/assay-core/src/engine/runner.rs" "std::process::Command" "runner process command"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "unwrap\\(|expect\\(" "mandate_store unwrap/expect (code-only)"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "\\bunsafe\\b" "mandate_store unsafe"
check_no_increase "crates/assay-core/src/runtime/mandate_store.rs" "tokio::spawn" "mandate_store tokio spawn"

echo "Wave2 Step1 reviewer script: PASS"
