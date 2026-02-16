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
judge_facade="crates/assay-core/src/judge/mod.rs"
judge_root="crates/assay-core/src/judge/judge_internal"
json_facade="crates/assay-evidence/src/json_strict/mod.rs"
json_root="crates/assay-evidence/src/json_strict/json_strict_internal"

if [ ! -d "${judge_root}" ]; then
  echo "Step3 precondition not met: missing ${judge_root}"
  exit 1
fi
if [ ! -d "${json_root}" ]; then
  echo "Step3 precondition not met: missing ${json_root}"
  exit 1
fi

check_has_match() {
  local pattern="$1"
  local file="$2"
  if ! "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_no_match() {
  local pattern="$1"
  local file="$2"
  if "$rg_bin" -n "$pattern" "$file" >/dev/null; then
    echo "forbidden match in ${file}: ${pattern}"
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

echo "== Wave7C Step3 quality checks =="
cargo fmt --check
cargo clippy -p assay-core -p assay-evidence --all-targets -- -D warnings
cargo check -p assay-core -p assay-evidence

echo "== Wave7C Step3 contract anchors (judge) =="
for test_name in \
  judge::judge_internal::tests::contract_two_of_three_majority \
  judge::judge_internal::tests::contract_sprt_early_stop \
  judge::judge_internal::tests::contract_abstain_mapping \
  judge::judge_internal::tests::contract_determinism_parallel_replay
 do
  echo "anchor: ${test_name}"
  cargo test -p assay-core --lib "${test_name}" -- --exact
 done

echo "== Wave7C Step3 contract anchors (json_strict) =="
for test_name in \
  json_strict::json_strict_internal::tests::test_rejects_top_level_duplicate \
  json_strict::json_strict_internal::tests::test_rejects_unicode_escape_duplicate \
  json_strict::json_strict_internal::tests::test_signature_duplicate_key_attack \
  json_strict::json_strict_internal::tests::test_dos_nesting_depth_limit \
  json_strict::json_strict_internal::tests::test_string_length_over_limit_rejected
 do
  echo "anchor: ${test_name}"
  cargo test -p assay-evidence --lib "${test_name}" -- --exact
 done

echo "== Wave7C Step3 blocked scope check =="
blocked_scope="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" '(^|/)Cargo\.toml$|(^|/)Cargo\.lock$|^\.github/workflows/' || true
)"
if [ -n "${blocked_scope}" ]; then
  echo "forbidden scope touched (Cargo/workflows):"
  echo "${blocked_scope}"
  exit 1
fi

echo "== Wave7C Step3 facade closure gates =="
check_has_match 'judge_internal::run::evaluate_impl' "${judge_facade}"
check_no_match '^\s*#\[cfg\(test\)\]' "${judge_facade}"
check_no_match '^\s*mod\s+tests\s*[{;]' "${judge_facade}"
check_no_match '^fn\s+' "${judge_facade}"
check_no_match 'serde_json::from_str|SYSTEM_PROMPT|build_prompt_impl|call_judge_impl|inject_result_impl|generate_cache_key_impl' "${judge_facade}"

check_has_match 'json_strict_internal::run::from_str_strict_impl' "${json_facade}"
check_has_match 'json_strict_internal::run::validate_json_strict_impl' "${json_facade}"
check_no_match '^\s*#\[cfg\(test\)\]' "${json_facade}"
check_no_match '^\s*mod\s+tests\s*[{;]' "${json_facade}"
check_no_match '^fn\s+' "${json_facade}"
check_no_match 'JsonValidator|parse_json_string_impl' "${json_facade}"

echo "== Wave7C Step3 single-source gates =="
check_only_file_matches \
  '\bfn\s+build_prompt_impl\b|\bconst\s+SYSTEM_PROMPT\b' \
  "${judge_root}" \
  'judge_internal/prompt.rs'
check_only_file_matches \
  '\basync\s+fn\s+call_judge_impl\b|\bserde_json::from_str\b' \
  "${judge_root}" \
  'judge_internal/client.rs'
check_only_file_matches \
  '\bfn\s+inject_result_impl\b|\bfn\s+generate_cache_key_impl\b' \
  "${judge_root}" \
  'judge_internal/cache.rs'
check_only_file_matches \
  '\basync\s+fn\s+evaluate_impl\b|\btriggers_rerun\s*\(' \
  "${judge_root}" \
  'judge_internal/run.rs'
check_only_file_matches \
  '^#\[(tokio::)?test\]|^async fn contract_|^fn contract_' \
  "${judge_root}" \
  'judge_internal/tests.rs'

check_only_file_matches \
  '\bimpl\s+JsonValidator\b|\bfn\s+validate_(value|object|array)\b' \
  "${json_root}" \
  'json_strict_internal/validate.rs'
check_only_file_matches \
  '\bfn\s+(parse_json_string_impl|decode_.*|unescape_.*)\b' \
  "${json_root}" \
  'json_strict_internal/decode.rs'
check_only_file_matches \
  '\bpub\(crate\)\s+use\s+crate::json_strict::errors::' \
  "${json_root}" \
  'json_strict_internal/limits.rs'
check_only_file_matches \
  '\bfn\s+from_str_strict_impl\b|\bfn\s+validate_json_strict_impl\b' \
  "${json_root}" \
  'json_strict_internal/run.rs'
check_only_file_matches \
  '^#\[test\]|^fn\s+test_' \
  "${json_root}" \
  'json_strict_internal/tests.rs'

echo "== Wave7C Step3 sensitive wording tripwires =="
check_has_match "expected ',' or '}'" "${json_root}/validate.rs"
check_has_match "expected ',' or ']'" "${json_root}/validate.rs"

echo "== Wave7C Step3 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-core/src/judge/mod.rs$|^crates/assay-core/src/judge/judge_internal/tests.rs$|^crates/assay-evidence/src/json_strict/mod.rs$|^crates/assay-evidence/src/json_strict/json_strict_internal/tests.rs$|^docs/contributing/SPLIT-CHECKLIST-wave7c-step3-judge-json-strict.md$|^docs/contributing/SPLIT-MOVE-MAP-wave7c-step3-judge-json-strict.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7c-step3-judge-json-strict.md$|^scripts/ci/review-wave7c-step3.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave7C Step3 reviewer script: PASS"
