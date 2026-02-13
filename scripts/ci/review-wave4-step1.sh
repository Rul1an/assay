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

echo "== Wave4 Step1 quality checks =="
echo "using base_ref=${base_ref}"
# Drift gates are conservative: false positives are acceptable, false negatives
# are possible until tests are externalized from hotspot files.
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo check -p assay-registry

lockfile_tests=(
  "test_lockfile_v2_roundtrip"
  "test_lockfile_stable_ordering"
  "test_lockfile_digest_mismatch_detection"
  "test_lockfile_signature_fields"
)

cache_tests=(
  "test_cache_roundtrip"
  "test_cache_integrity_failure"
  "test_signature_json_corrupt_handling"
  "test_atomic_write_prevents_partial_cache"
)

echo "== Wave4 Step1 contract freeze tests (lockfile) =="
printf '%s\n' "${lockfile_tests[@]}" | sed 's/^/anchor: /'
for test_name in "${lockfile_tests[@]}"; do
  cargo test -p assay-registry "${test_name}" -- --nocapture
done

echo "== Wave4 Step1 contract freeze tests (cache) =="
printf '%s\n' "${cache_tests[@]}" | sed 's/^/anchor: /'
for test_name in "${cache_tests[@]}"; do
  cargo test -p assay-registry "${test_name}" -- --nocapture
done

lockfile_file="crates/assay-registry/src/lockfile.rs"
cache_file="crates/assay-registry/src/cache.rs"

echo "== Wave4 Step1 drift gates =="
check_no_increase "$lockfile_file" "unwrap\\(|expect\\(" "lockfile unwrap/expect (best-effort code-only)"
check_no_increase "$lockfile_file" "\\bunsafe\\b" "lockfile unsafe"
check_no_increase "$lockfile_file" "println!\\(|eprintln!\\(" "lockfile println/eprintln (best-effort code-only)"
check_no_increase "$lockfile_file" "dbg!\\(|trace!\\(|debug!\\(" "lockfile dbg/trace/debug (best-effort code-only)"
check_no_increase "$lockfile_file" "panic!\\(|todo!\\(|unimplemented!\\(" "lockfile panic/todo/unimplemented (best-effort code-only)"
check_no_increase "$cache_file" "unwrap\\(|expect\\(" "cache unwrap/expect (best-effort code-only)"
check_no_increase "$cache_file" "\\bunsafe\\b" "cache unsafe"
check_no_increase "$cache_file" "println!\\(|eprintln!\\(" "cache println/eprintln (best-effort code-only)"
check_no_increase "$cache_file" "dbg!\\(|trace!\\(|debug!\\(" "cache dbg/trace/debug (best-effort code-only)"
check_no_increase "$cache_file" "OpenOptions|tempfile|rename\\(|fs::|std::fs" "cache filesystem/helper surface (best-effort code-only)"
check_no_increase "$cache_file" "panic!\\(|todo!\\(|unimplemented!\\(" "cache panic/todo/unimplemented (best-effort code-only)"

echo "== Wave4 Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      "^crates/assay-registry/src/lockfile.rs$|^crates/assay-registry/src/cache.rs$|^docs/contributing/SPLIT-INVENTORY-wave4-step1.md$|^docs/contributing/SPLIT-SYMBOLS-wave4-step1.md$|^docs/contributing/SPLIT-CHECKLIST-lockfile-step1.md$|^docs/contributing/SPLIT-CHECKLIST-cache-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave4-step1.md$|^scripts/ci/review-wave4-step1.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$" || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave4 Step1 reviewer script: PASS"
