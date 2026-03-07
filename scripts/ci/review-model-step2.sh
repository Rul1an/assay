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

echo '== Model Step2 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact
cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact
cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact

echo '== Model Step2 scope checks =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^crates/assay-core/src/model\.rs$|^crates/assay-core/src/model/|^docs/contributing/SPLIT-CHECKLIST-model-step2\.md$|^docs/contributing/SPLIT-MOVE-MAP-model-step2\.md$|^docs/contributing/SPLIT-REVIEW-PACK-model-step2\.md$|^scripts/ci/review-model-step2\.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in model Step2'
  exit 1
fi

if git status --porcelain -- crates/assay-core/src/model | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/model/** are forbidden in Step2'
  exit 1
fi

echo '== Model Step2 facade invariants =='
facade='crates/assay-core/src/model/mod.rs'
if [ ! -f "${facade}" ]; then
  echo "missing facade file: ${facade}"
  exit 1
fi

facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 220 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 220): ${facade}"
  exit 1
fi

"${rg_bin}" -n '^mod types;\s*$' "${facade}" >/dev/null || { echo "missing 'mod types;'"; exit 1; }
"${rg_bin}" -n '^mod serde;\s*$' "${facade}" >/dev/null || { echo "missing 'mod serde;'"; exit 1; }
"${rg_bin}" -n '^mod validation;\s*$' "${facade}" >/dev/null || { echo "missing 'mod validation;'"; exit 1; }
"${rg_bin}" -n '^#\[cfg\(test\)\]\s*$' "${facade}" >/dev/null || { echo 'missing #[cfg(test)] in facade'; exit 1; }
"${rg_bin}" -n '^mod tests;\s*$' "${facade}" >/dev/null || { echo "missing 'mod tests;'"; exit 1; }
"${rg_bin}" -n '^pub\s+use\s+types::' "${facade}" >/dev/null || { echo 'facade must re-export types via pub use types::'; exit 1; }

if "${rg_bin}" -n '^\s*fn\s+\w+' "${facade}" >/dev/null; then
  echo "facade must not define functions: ${facade}"
  "${rg_bin}" -n '^\s*fn\s+\w+' "${facade}"
  exit 1
fi

if "${rg_bin}" -n 'std::fs|read_to_string|PathBuf|env::' "${facade}" >/dev/null; then
  echo "facade contains IO/env markers: ${facade}"
  exit 1
fi

echo '== Model Step2 boundary invariants =='
check_no_io() {
  local f="$1"
  if [ ! -f "${f}" ]; then
    echo "missing module file: ${f}"
    exit 1
  fi
  if "${rg_bin}" -n 'std::fs|tokio::fs|read_to_string|File|OpenOptions|create_dir|PathBuf|env::|reqwest' "${f}" >/dev/null; then
    echo "IO/surprising deps not allowed in ${f}"
    "${rg_bin}" -n 'std::fs|tokio::fs|read_to_string|File|OpenOptions|create_dir|PathBuf|env::|reqwest' "${f}"
    exit 1
  fi
}

check_no_io 'crates/assay-core/src/model/serde.rs'
check_no_io 'crates/assay-core/src/model/validation.rs'

echo '== Model Step2 relocation invariants =='
if [ -f 'crates/assay-core/src/model.rs' ]; then
  echo 'legacy crates/assay-core/src/model.rs should be removed after split'
  exit 1
fi

if "${rg_bin}" -n '^\s*fn\s+test_' 'crates/assay-core/src/model/mod.rs' >/dev/null; then
  echo 'tests must not remain in facade'
  exit 1
fi

test_count="$("${rg_bin}" -n '^\s*fn\s+test_' 'crates/assay-core/src/model/tests/mod.rs' | wc -l | tr -d ' ')"
if [ "${test_count}" -lt 5 ]; then
  echo "expected at least 5 relocated tests in model/tests/mod.rs, got ${test_count}"
  exit 1
fi

echo 'Model Step2 reviewer script: PASS'
