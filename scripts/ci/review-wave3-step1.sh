#!/usr/bin/env bash
set -euo pipefail

base_ref="${1:-origin/main}"
rg_bin="$(command -v rg)"

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

count_in_ref() {
  local ref="$1"
  local file="$2"
  local pattern="$3"
  git show "${ref}:${file}" | strip_code_only /dev/stdin | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local file="$1"
  local pattern="$2"
  strip_code_only "$file" | "$rg_bin" -n "$pattern" || true
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

echo "== Wave3 Step1 quality checks =="
cargo fmt --check
cargo clippy -p assay-cli -p assay-core --all-targets -- -D warnings
cargo check -p assay-cli -p assay-core

echo "== Wave3 Step1 contract tests (monitor) =="
if [ "$(uname -s)" = "Linux" ]; then
  cargo test -p assay-cli test_kernel_dev_encoding_overflow -- --nocapture
  cargo test -p assay-cli test_normalize_path_syntactic_contract -- --nocapture
  cargo test -p assay-cli test_find_violation_rule_allow_not_contract -- --nocapture
else
  cargo test -p assay-cli test_normalize_path_syntactic_contract_skip_non_linux -- --nocapture
  cargo test -p assay-cli test_find_violation_rule_allow_not_contract_skip_non_linux -- --nocapture
fi

echo "== Wave3 Step1 contract tests (trace) =="
cargo test -p assay-core --lib test_from_path_invalid_json_has_line_context -- --nocapture
cargo test -p assay-core --lib test_v2_non_model_prompt_is_only_fallback -- --nocapture
cargo test -p assay-core --lib test_from_path_accepts_crlf_jsonl_lines -- --nocapture

echo "== Wave3 Step1 drift gates =="
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "unwrap\\(|expect\\(" "monitor unwrap/expect (code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "\\bunsafe\\b" "monitor unsafe"
check_no_increase "crates/assay-core/src/providers/trace.rs" "unwrap\\(|expect\\(" "trace unwrap/expect (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "\\bunsafe\\b" "trace unsafe"

echo "Wave3 Step1 reviewer script: PASS"
