#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/codex/wave3-step1-behavior-freeze-v2"
fi

rg_bin="$(command -v rg)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

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

check_no_match() {
  local pattern="$1"
  local path="$2"
  if "$rg_bin" -n "$pattern" "$path"; then
    echo "forbidden match in $path (pattern: $pattern)"
    exit 1
  fi
}

check_no_match_in_dir_excluding() {
  local pattern="$1"
  local root="$2"
  local excluded_file="$3"
  local matches
  matches="$($rg_bin -n "$pattern" "$root" -g'*.rs' -g"!${excluded_file}" || true)"
  if [ -n "$matches" ]; then
    echo "forbidden matches outside ${excluded_file}:"
    echo "$matches"
    exit 1
  fi
}

check_only_file_matches() {
  local pattern="$1"
  local root="$2"
  local allowed="$3"
  local matches leaked
  matches="$($rg_bin -n "$pattern" "$root" -g'*.rs' || true)"
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

echo "== Wave3 Step2 quality checks =="
echo "using base_ref=${base_ref}"
cargo fmt --check
cargo clippy -p assay-cli -p assay-core --all-targets -- -D warnings
cargo check -p assay-cli -p assay-core

echo "== Wave3 Step2 contract tests (Step1 freeze set) =="
if [ "$(uname -s)" = "Linux" ]; then
  cargo test -p assay-cli test_normalize_path_syntactic_contract -- --nocapture
  cargo test -p assay-cli test_find_violation_rule_allow_not_contract -- --nocapture
else
  cargo test -p assay-cli test_normalize_path_syntactic_contract_skip_non_linux -- --nocapture
  cargo test -p assay-cli test_find_violation_rule_allow_not_contract_skip_non_linux -- --nocapture
fi
cargo test -p assay-core --lib test_from_path_invalid_json_has_line_context -- --nocapture
cargo test -p assay-core --lib test_v2_non_model_prompt_is_only_fallback -- --nocapture
cargo test -p assay-core --lib test_from_path_accepts_crlf_jsonl_lines -- --nocapture

echo "== Wave3 Step2 drift gates (Step1 continuity) =="
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "unwrap\\(|expect\\(" "monitor unwrap/expect (best-effort code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "\\bunsafe\\b" "monitor unsafe"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "println!\\(|eprintln!\\(" "monitor println/eprintln (best-effort code-only)"
check_no_increase "crates/assay-cli/src/cli/commands/monitor.rs" "panic!\\(|todo!\\(|unimplemented!\\(" "monitor panic/todo/unimplemented (best-effort code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "unwrap\\(|expect\\(" "trace unwrap/expect (best-effort code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "\\bunsafe\\b" "trace unsafe"
check_no_increase "crates/assay-core/src/providers/trace.rs" "println!\\(|eprintln!\\(" "trace println/eprintln (best-effort code-only)"
check_no_increase "crates/assay-core/src/providers/trace.rs" "panic!\\(|todo!\\(|unimplemented!\\(" "trace panic/todo/unimplemented (best-effort code-only)"

echo "== Wave3 Step2 boundary gates =="
strip_code_only crates/assay-cli/src/cli/commands/monitor.rs > "${tmp_dir}/monitor_facade.rs"
strip_code_only crates/assay-core/src/providers/trace.rs > "${tmp_dir}/trace_facade.rs"

# Monitor facade stays thin and delegates only.
check_no_match "globset|nix::|libc|syscall|\\bbpf\\b|MonitorEvent|kill_pid|decode_utf8_cstr|dump_prefix_hex" "${tmp_dir}/monitor_facade.rs"
check_no_match "println!\\(|eprintln!\\(|tracing::(info|warn|error)!" "${tmp_dir}/monitor_facade.rs"

# Unsafe containment: only syscall_linux.rs may use unsafe.
check_only_file_matches "unsafe[[:space:]]*\\{|unsafe[[:space:]]+fn" \
  crates/assay-cli/src/cli/commands/monitor_next \
  "monitor_next/syscall_linux.rs"

# Output containment: printing must be routed to output.rs.
check_no_match_in_dir_excluding "println!\\(|eprintln!\\(" \
  crates/assay-cli/src/cli/commands/monitor_next \
  "output.rs"
check_no_match "println!\\(|eprintln!\\(" crates/assay-cli/src/cli/commands/monitor_next/syscall_linux.rs

# Trace facade stays thin and serde/io work remains in trace_next/*.
check_no_match "serde_json::|simd_json::|BufRead|read_line|lines\\(|fs::|File|OpenOptions" "${tmp_dir}/trace_facade.rs"
check_no_match "parse_|normalize_|v2_|EpisodeState|ParsedTraceRecord" "${tmp_dir}/trace_facade.rs"

# JSON parsing stays in trace parsing modules.
check_only_file_matches "serde_json::from_str" \
  crates/assay-core/src/providers/trace_next \
  "trace_next/(parse|v2).rs"

echo "== Wave3 Step2 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      "^crates/assay-cli/src/cli/commands/monitor.rs$|^crates/assay-cli/src/cli/commands/monitor_next/|^crates/assay-core/src/providers/trace.rs$|^crates/assay-core/src/providers/trace_next/|^docs/contributing/SPLIT-CHECKLIST-wave3-step2.md$|^docs/contributing/SPLIT-MOVE-MAP-wave3-step2.md$|^scripts/ci/review-wave3-step2.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$" || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave3 Step2 reviewer script: PASS"
