#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-origin/codex/wave2-step2-runtime-split}}"
rg_bin="$(command -v rg)"

strip_code_only() {
  local file="$1"
  # Best-effort filter:
  # skips `#[cfg(test)] mod tests { ... }` blocks, but does not parse arbitrary
  # cfg-gated test item layouts outside a tests module.
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
  git show "${ref}:${file}" | strip_code_only /dev/stdin | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
}

count_in_worktree() {
  local file="$1"
  local pattern="$2"
  strip_code_only "$file" | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" || true
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
echo "using base_ref=${base_ref}"
# Drift gates are conservative: false positives are acceptable, false negatives
# are possible until tests are externalized from hotspot files.
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
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "println!\\(|eprintln!\\(" "monitor println/eprintln (code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "panic!\\(|todo!\\(|unimplemented!\\(" "monitor panic/todo/unimplemented (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "unwrap\\(|expect\\(" "trace unwrap/expect (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "\\bunsafe\\b" "trace unsafe"
check_no_increase "crates/assay-core/src/providers/trace.rs" "println!\\(|eprintln!\\(" "trace println/eprintln (code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "panic!\\(|todo!\\(|unimplemented!\\(" "trace panic/todo/unimplemented (code-only)"

echo "== Wave3 Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      "^crates/assay-cli/src/cli/commands/monitor.rs$|^crates/assay-core/src/providers/trace.rs$|^docs/contributing/SPLIT-INVENTORY-wave3-step1.md$|^docs/contributing/SPLIT-CHECKLIST-monitor-step1.md$|^docs/contributing/SPLIT-CHECKLIST-trace-step1.md$|^docs/contributing/SPLIT-SYMBOLS-wave3-step1.md$|^scripts/ci/review-wave3-step1.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$" || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave3 Step1 reviewer script: PASS"
