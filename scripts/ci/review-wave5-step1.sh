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

echo "== Wave5 Step1 quality checks =="
cargo fmt --check
cargo clippy -p assay-registry --all-targets -- -D warnings
cargo check -p assay-registry

echo "== Wave5 Step1 contract anchors (verify) =="
verify_tests=(
  "test_verify_pack_fail_closed_matrix_contract"
  "test_verify_pack_malformed_signature_reason_is_stable"
  "test_verify_pack_canonicalization_equivalent_yaml_variants_contract"
  "test_verify_pack_uses_canonical_bytes"
  "test_verify_digest_mismatch"
  "test_parse_dsse_envelope_invalid_base64"
)
printf '%s\n' "${verify_tests[@]}" | sed 's/^/anchor: /'
for test_name in "${verify_tests[@]}"; do
  cargo test -p assay-registry "$test_name" -- --nocapture
done

echo "== Wave5 Step1 no-production-change gate =="
verify_path="crates/assay-registry/src/verify.rs"
git show "${base_ref}:${verify_path}" | strip_code_only /dev/stdin > "${tmp_dir}/verify_base_code.rs"
strip_code_only "${verify_path}" > "${tmp_dir}/verify_head_code.rs"
if ! cmp -s "${tmp_dir}/verify_base_code.rs" "${tmp_dir}/verify_head_code.rs"; then
  echo "verify.rs production code changed in Step1; only #[cfg(test)] changes are allowed"
  diff -u "${tmp_dir}/verify_base_code.rs" "${tmp_dir}/verify_head_code.rs" | sed -n '1,120p'
  exit 1
fi

echo "== Wave5 Step1 public-surface freeze gate =="
git show "${base_ref}:${verify_path}" | "$rg_bin" -n '^pub (fn|struct|enum|type|const)' > "${tmp_dir}/verify_pub_base.txt"
"$rg_bin" -n '^pub (fn|struct|enum|type|const)' "${verify_path}" > "${tmp_dir}/verify_pub_head.txt"
if ! cmp -s "${tmp_dir}/verify_pub_base.txt" "${tmp_dir}/verify_pub_head.txt"; then
  echo "verify public surface drift detected"
  diff -u "${tmp_dir}/verify_pub_base.txt" "${tmp_dir}/verify_pub_head.txt"
  exit 1
fi

echo "== Wave5 Step1 drift gates =="
check_no_increase "${verify_path}" 'unwrap\(|expect\(' 'verify unwrap/expect (best-effort code-only)'
check_no_increase "${verify_path}" '\bunsafe\b' 'verify unsafe'
check_no_increase "${verify_path}" 'println!\(|eprintln!\(' 'verify println/eprintln (best-effort code-only)'
check_no_increase "${verify_path}" 'panic!\(|todo!\(|unimplemented!\(' 'verify panic/todo/unimplemented (best-effort code-only)'
check_no_increase "${verify_path}" 'dbg!\(|tracing::(trace|debug)!' 'verify dbg/trace/debug (best-effort code-only)'

echo "== Wave5 Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-registry/src/verify.rs$|^docs/contributing/SPLIT-INVENTORY-wave5-step1-verify.md$|^docs/contributing/SPLIT-SYMBOLS-wave5-step1-verify.md$|^docs/contributing/SPLIT-CHECKLIST-verify-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave5-step1.md$|^scripts/ci/review-wave5-step1.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave5 Step1 reviewer script: PASS"
