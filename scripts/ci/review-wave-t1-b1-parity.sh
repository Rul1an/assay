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
root_file="crates/assay-core/tests/parity.rs"
split_dir="crates/assay-core/tests/parity"

count_delta_no_increase() {
  local pattern="$1"
  local label="$2"
  local before after
  before="$({ git show "${base_ref}:${root_file}" | "${rg_bin}" -n "${pattern}" || true; } | wc -l | tr -d ' ')"
  after="$({ "${rg_bin}" -n "${pattern}" "${root_file}" "${split_dir}" -g '*.rs' || true; } | wc -l | tr -d ' ')"
  echo "${label}: before=${before} after=${after}"
  if [ "${after}" -gt "${before}" ]; then
    echo "drift gate failed: ${label} increased"
    exit 1
  fi
}

echo "== Wave T1 B1 quality checks =="
cargo fmt --check
cargo clippy -p assay-core --tests -- -D warnings
cargo test -p assay-core --test parity

echo "== Wave T1 B1 single-source checks =="
if [ "$("${rg_bin}" -n '^pub fn compute_result_hash\(' "${root_file}" "${split_dir}" -g '*.rs' | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "compute_result_hash must be single-source"
  exit 1
fi
if [ "$("${rg_bin}" -n '^pub fn verify_parity\(' "${root_file}" "${split_dir}" -g '*.rs' | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "verify_parity must be single-source"
  exit 1
fi

echo "== Wave T1 B1 test-name anchors =="
for name in test_all_parity test_args_valid_parity test_sequence_parity test_blocklist_parity test_hash_determinism; do
  if ! "${rg_bin}" -n "^fn ${name}\(" "${split_dir}/parity_contract.rs" >/dev/null; then
    echo "missing expected test function: ${name}"
    exit 1
  fi
done

echo "== Wave T1 B1 drift gates =="
count_delta_no_increase 'unwrap\(|expect\(' 'unwrap/expect'
count_delta_no_increase '\bunsafe\b' 'unsafe'
count_delta_no_increase 'println!\(|eprintln!\(|print!\(|dbg!\(' 'print/debug'
count_delta_no_increase 'panic!\(|todo!\(|unimplemented!\(' 'panic/todo/unimplemented'

echo "== Wave T1 B1 diff allowlist =="
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-core/tests/parity.rs$|^crates/assay-core/tests/parity/|^docs/contributing/SPLIT-CHECKLIST-wave-t1-b1-parity.md$|^docs/contributing/SPLIT-MOVE-MAP-wave-t1-b1-parity.md$|^docs/contributing/SPLIT-REVIEW-PACK-wave-t1-b1-parity.md$|^scripts/ci/review-wave-t1-b1-parity.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo "workflow changes are forbidden in Wave T1 B1"
  exit 1
fi

echo "Wave T1 B1 reviewer script: PASS"
