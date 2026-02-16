#!/usr/bin/env bash
set -euo pipefail

base_ref="${BASE_REF:-${1:-}}"
if [ -z "${base_ref}" ] && [ -n "${GITHUB_BASE_REF:-}" ]; then
  base_ref="origin/${GITHUB_BASE_REF}"
fi
if [ -z "${base_ref}" ]; then
  base_ref="origin/main"
fi
if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}"
  exit 1
fi
echo "BASE_REF=${base_ref} sha=$(git rev-parse "${base_ref}")"
echo "HEAD sha=$(git rev-parse HEAD)"

rg_bin="$(command -v rg)"
tmp_dir="$(mktemp -d)"
trap 'rm -rf "${tmp_dir}"' EXIT

judge_facade="crates/assay-core/src/judge/mod.rs"
judge_root="crates/assay-core/src/judge/judge_internal"
json_facade="crates/assay-evidence/src/json_strict/mod.rs"
json_root="crates/assay-evidence/src/json_strict/json_strict_internal"

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
        if (line ~ /^[[:space:]]*mod[[:space:]]+tests[[:space:]]*;[[:space:]]*$/) {
          pending_cfg_test = 0
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

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_has_any_match() {
  local file="$1"
  shift
  local pattern
  for pattern in "$@"; do
    if "$rg_bin" -n "$pattern" "$file" >/dev/null; then
      return 0
    fi
  done
  echo "missing expected any-pattern in ${file}: $*"
  exit 1
}

check_no_match_code_only() {
  local pattern="$1"
  local file="$2"
  if strip_code_only "$file" | "$rg_bin" -v '^[[:space:]]*//' | "$rg_bin" -n "$pattern" >/dev/null; then
    echo "forbidden code-only match in ${file}: ${pattern}"
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

if [ ! -d "${judge_root}" ]; then
  echo "Step2 precondition not met: missing ${judge_root}"
  exit 1
fi
if [ ! -d "${json_root}" ]; then
  echo "Step2 precondition not met: missing ${json_root}"
  exit 1
fi

echo "== Wave7C Step2 quality checks =="
cargo fmt --check
cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-core -p assay-evidence

echo "== Wave7C Step2 contract anchors (judge) =="
for test_name in \
  judge::tests::contract_two_of_three_majority \
  judge::tests::contract_sprt_early_stop \
  judge::tests::contract_abstain_mapping \
  judge::tests::contract_determinism_parallel_replay
do
  echo "anchor: ${test_name}"
  cargo test -p assay-core --lib "${test_name}" -- --exact
done

echo "== Wave7C Step2 contract anchors (json_strict) =="
for test_name in \
  json_strict::tests::test_rejects_top_level_duplicate \
  json_strict::tests::test_rejects_unicode_escape_duplicate \
  json_strict::tests::test_signature_duplicate_key_attack \
  json_strict::tests::test_dos_nesting_depth_limit \
  json_strict::tests::test_string_length_over_limit_rejected
do
  echo "anchor: ${test_name}"
  cargo test -p assay-evidence --lib "${test_name}" -- --exact
done

echo "== Wave7C Step2 blocked scope check =="
blocked_scope="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" '(^|/)Cargo\.toml$|(^|/)Cargo\.lock$|^\.github/workflows/' || true
)"
if [ -n "${blocked_scope}" ]; then
  echo "forbidden scope touched (Cargo/workflows):"
  echo "${blocked_scope}"
  exit 1
fi

echo "== Wave7C Step2 facade containment gates =="
check_no_match_code_only \
  'tokio::fs|std::fs|OpenOptions|Command::new|std::process|tokio::process|reqwest|hyper|Sha256|hex::|serde_yaml::|globset::(Glob|GlobSet|GlobSetBuilder)' \
  "${judge_facade}"
check_no_match_code_only \
  'tokio::fs|std::fs|OpenOptions|Command::new|std::process|tokio::process|reqwest|hyper|Sha256|hex::|serde_yaml::' \
  "${json_facade}"

check_has_any_match "${judge_facade}" \
  'judge_internal::run::evaluate_impl' \
  'judge_internal::run::evaluate'
check_has_any_match "${json_facade}" \
  'json_strict_internal::validate::from_str_strict_impl' \
  'json_strict_internal::run::from_str_strict_impl'
check_has_any_match "${json_facade}" \
  'json_strict_internal::validate::validate_json_strict_impl' \
  'json_strict_internal::run::validate_json_strict_impl'

echo "== Wave7C Step2 single-source gates =="
check_only_file_matches \
  '\bfn\s+build_prompt_impl\b|\bconst\s+SYSTEM_PROMPT\b' \
  "${judge_root}" \
  'judge_internal/prompt.rs'
check_only_file_matches \
  '\basync\s+fn\s+call_judge_impl\b|\bserde_json::from_str\b|\bLlmClient\b' \
  "${judge_root}" \
  'judge_internal/client.rs'
check_only_file_matches \
  '\basync\s+fn\s+evaluate_impl\b|\btriggers_rerun\s*\(|\binject_result_impl\s*\(|\bgenerate_cache_key_impl\s*\(' \
  "${judge_root}" \
  'judge_internal/run.rs'

check_only_file_matches \
  '\bimpl\s+JsonValidator\b|\bfn\s+validate_(value|object|array)\b' \
  "${json_root}" \
  'json_strict_internal/validate.rs'
check_only_file_matches \
  '\bfn\s+(parse_json_string|decode_.*|unescape_.*)\b|\bsurrogate\b' \
  "${json_root}" \
  'json_strict_internal/decode.rs'
check_only_file_matches \
  '\b(MAX_NESTING_DEPTH|MAX_KEYS_PER_OBJECT|MAX_STRING_LENGTH)\b' \
  "${json_root}" \
  'json_strict_internal/limits.rs'

echo "== Wave7C Step2 sensitive wording tripwires =="
check_has_match "expected ',' or '}'" "${json_root}/validate.rs"
check_has_match "expected ',' or ']'" "${json_root}/validate.rs"

echo "== Wave7C Step2 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-core/src/judge/mod.rs$|^crates/assay-core/src/judge/judge_internal/|^crates/assay-evidence/src/json_strict/mod.rs$|^crates/assay-evidence/src/json_strict/json_strict_internal/|^docs/contributing/SPLIT-CHECKLIST-wave7c-step2-judge-json-strict.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7c-step2-judge-json-strict.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7c-step2-judge-json-strict.md$|^scripts/ci/review-wave7c-step2.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave7C Step2 reviewer script: PASS"
