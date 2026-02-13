# Runner split Step 1 checklist (behavior freeze)

Scope lock:
- tests + docs + gates only
- no mechanical split yet
- no perf tuning
- `demo/` untouched

## Contract targets

- status/result mapping remains stable (`pass`, `fail`, `flaky`, `allowed`)
- retry accounting stays stable (`attempt`/`max_retries`)
- relative baseline helper warning behavior stays stable
- progress sink totals stay stable (`done`/`total`)

## Drift gates (hard-fail)

Run with `bash` + `set -euo pipefail`. Counts are code-only (exclude `#[cfg(test)]` block).

```bash
set -euo pipefail

base_ref="origin/main"
file="crates/assay-core/src/engine/runner.rs"
rg_bin="$(command -v rg)"

count_in_ref() {
  local pattern="$1"
  git show "${base_ref}:${file}" | awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local pattern="$1"
  awk 'BEGIN{in_tests=0} /^#\[cfg\(test\)\]/{in_tests=1} {if(!in_tests) print}' "$file" | "$rg_bin" -n "$pattern" || true
}

check_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$(count_in_ref "$pattern" | wc -l | tr -d ' ')"
  after="$(count_in_worktree "$pattern" | wc -l | tr -d ' ')"
  echo "$label: before=$before after=$after"
  if [ "$after" -gt "$before" ]; then
    echo "drift gate failed: $label increased"
    exit 1
  fi
}

check_no_increase "unwrap\\(|expect\\(" "runner unwrap/expect (code-only)"
check_no_increase "\\bunsafe\\b" "runner unsafe"
check_no_increase "println!|eprintln!" "runner stdout/stderr"
check_no_increase "std::process::Command" "runner process command"
```

## Required contract tests

```bash
cargo test -p assay-core --lib runner_contract_flake_fail_then_pass_classified_flaky -- --nocapture
cargo test -p assay-core --lib runner_contract_fail_after_retries_stays_fail -- --nocapture
cargo test -p assay-core --lib runner_contract_on_error_allow_marks_allowed_and_policy_applied -- --nocapture
cargo test -p assay-core --lib runner_contract_results_sorted_by_test_id -- --nocapture
cargo test -p assay-core --lib runner_contract_progress_sink_reports_done_total -- --nocapture
cargo test -p assay-core --lib runner_contract_relative_baseline_missing_warns_in_helper -- --nocapture
```

## Definition of done

- no drift-gate increases in `runner.rs`
- all runner Step 1 contract tests pass
- scope lock respected
