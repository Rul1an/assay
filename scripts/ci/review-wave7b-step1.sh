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

echo "== Wave7B Step1 quality checks =="
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo clippy -p assay-core --all-targets -- -D warnings
cargo check -p assay-evidence -p assay-core

echo "== Wave7B Step1 contract anchors (loader) =="
for test_name in \
  test_local_pack_resolution \
  test_builtin_wins_over_local \
  test_local_invalid_yaml_fails \
  test_resolution_order_mock \
  test_path_wins_over_builtin \
  test_symlink_escape_rejected
do
  echo "anchor: ${test_name}"
  cargo test -p assay-evidence "${test_name}" -- --nocapture
done

echo "== Wave7B Step1 contract anchors (store) =="
echo "anchor: test_storage_smoke_lifecycle"
cargo test -p assay-core --test storage_smoke test_storage_smoke_lifecycle -- --nocapture

for test_name in \
  e1_runs_write_contract_insert_and_create \
  e1_latest_run_selection_is_id_based_not_timestamp_string \
  e1_stats_read_compat_keeps_legacy_started_at
do
  echo "anchor: ${test_name}"
  cargo test -p assay-core --test store_consistency_e1 "${test_name}" -- --nocapture
done

echo "== Wave7B Step1 no-production-change gates =="
hotspot_paths=(
  "crates/assay-evidence/src/lint/packs/loader.rs"
  "crates/assay-core/src/storage/store.rs"
)
for hotspot in "${hotspot_paths[@]}"; do
  git show "${base_ref}:${hotspot}" | strip_code_only /dev/stdin > "${tmp_dir}/base_code.rs"
  strip_code_only "${hotspot}" > "${tmp_dir}/head_code.rs"
  if ! cmp -s "${tmp_dir}/base_code.rs" "${tmp_dir}/head_code.rs"; then
    echo "${hotspot} production code changed in Step1; only #[cfg(test)] changes are allowed"
    diff -u "${tmp_dir}/base_code.rs" "${tmp_dir}/head_code.rs" | sed -n '1,120p'
    exit 1
  fi
done

echo "== Wave7B Step1 public-surface freeze gates =="
for hotspot in "${hotspot_paths[@]}"; do
  git show "${base_ref}:${hotspot}" | "$rg_bin" -n '^\s*pub\s+(const|struct|enum|type|trait|fn)\b' > "${tmp_dir}/pub_base.txt"
  "$rg_bin" -n '^\s*pub\s+(const|struct|enum|type|trait|fn)\b' "${hotspot}" > "${tmp_dir}/pub_head.txt"
  if ! cmp -s "${tmp_dir}/pub_base.txt" "${tmp_dir}/pub_head.txt"; then
    echo "${hotspot} public surface drift detected"
    diff -u "${tmp_dir}/pub_base.txt" "${tmp_dir}/pub_head.txt"
    exit 1
  fi
done

echo "== Wave7B Step1 drift gates =="
for hotspot in "${hotspot_paths[@]}"; do
  check_no_increase "${hotspot}" 'unwrap\(|expect\(' "${hotspot} unwrap/expect (best-effort code-only)"
  check_no_increase "${hotspot}" '\bunsafe\b' "${hotspot} unsafe"
  check_no_increase "${hotspot}" 'println!\(|eprintln!\(|print!\(|dbg!\(|tracing::(debug|trace)!' "${hotspot} print/debug/log (best-effort code-only)"
  check_no_increase "${hotspot}" 'panic!\(|todo!\(|unimplemented!\(' "${hotspot} panic/todo/unimplemented (best-effort code-only)"
  check_no_increase "${hotspot}" 'tokio::fs|std::fs|OpenOptions|rename\(|create_dir_all|tempfile' "${hotspot} IO footprint (best-effort code-only)"
  check_no_increase "${hotspot}" 'Command::new|std::process|tokio::process|reqwest|hyper' "${hotspot} process/network (best-effort code-only)"
done

echo "== Wave7B Step1 diff allowlist =="
leaks="$(
  git diff --name-only "${base_ref}...HEAD" | \
    "$rg_bin" -v \
      '^crates/assay-evidence/src/lint/packs/loader.rs$|^crates/assay-core/src/storage/store.rs$|^docs/contributing/SPLIT-INVENTORY-wave7b-step1-loader-store.md$|^docs/contributing/SPLIT-SYMBOLS-wave7b-step1-loader-store.md$|^docs/contributing/SPLIT-CHECKLIST-wave7b-step1-loader-store.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave7b-step1-loader-store.md$|^scripts/ci/review-wave7b-step1.sh$|^docs/architecture/PLAN-split-refactor-2026q1.md$' || true
)"
if [ -n "$leaks" ]; then
  echo "non-allowlisted files detected:"
  echo "$leaks"
  exit 1
fi

echo "Wave7B Step1 reviewer script: PASS"
