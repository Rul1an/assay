# Mandate store split Step 1 checklist (behavior freeze)

Scope lock:
- tests + docs + gates only
- no mechanical split yet
- no perf tuning
- `demo/` untouched

## Contract targets

- idempotent consume behavior for same `tool_call_id` remains stable
- monotonic use-count behavior for new `tool_call_id`s remains stable
- revocation and nonce semantics remain stable
- `compute_use_id` vector remains stable

## Drift gates (hard-fail)

Run with `bash` + `set -euo pipefail`. Counts are code-only (exclude `#[cfg(test)]` block).

```bash
set -euo pipefail

base_ref="origin/main"
file="crates/assay-core/src/runtime/mandate_store.rs"
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

check_no_increase "unwrap\\(|expect\\(" "mandate_store unwrap/expect (code-only)"
check_no_increase "\\bunsafe\\b" "mandate_store unsafe"
check_no_increase "tokio::spawn" "mandate_store tokio spawn"
```

## Required contract tests

```bash
cargo test -p assay-core --lib test_compute_use_id_contract_vector -- --nocapture
cargo test -p assay-core --lib test_multicall_produces_monotonic_counts_no_gaps -- --nocapture
cargo test -p assay-core --lib test_multicall_idempotent_same_tool_call_id -- --nocapture
cargo test -p assay-core --test mandate_store_concurrency test_two_connections_same_tool_call_id_has_single_new_receipt -- --nocapture
```

## Definition of done

- no drift-gate increases in `mandate_store.rs`
- mandate store contract tests pass (including parallel regression)
- scope lock respected
