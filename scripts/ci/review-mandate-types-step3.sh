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
facade='crates/assay-evidence/src/mandate/types/mod.rs'
core_file='crates/assay-evidence/src/mandate/types/core.rs'
serde_file='crates/assay-evidence/src/mandate/types/serde.rs'
schema_file='crates/assay-evidence/src/mandate/types/schema.rs'
tests_file='crates/assay-evidence/src/mandate/types/tests.rs'

echo '== Mandate types Step3 quality checks =='
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings
cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_kind_serialization -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_builder -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_operation_class_serialization -- --exact

echo '== Mandate types Step3 invariants =='
if [ ! -f "${facade}" ]; then
  echo "missing facade file: ${facade}"
  exit 1
fi

facade_loc="$(awk 'NF{c++} END{print c+0}' "${facade}")"
if [ "${facade_loc}" -gt 220 ]; then
  echo "facade LOC budget exceeded (${facade_loc} > 220): ${facade}"
  exit 1
fi

"${rg_bin}" -n '^mod core;\s*$' "${facade}" >/dev/null || { echo "missing 'mod core;'"; exit 1; }
"${rg_bin}" -n '^mod schema;\s*$' "${facade}" >/dev/null || { echo "missing 'mod schema;'"; exit 1; }
"${rg_bin}" -n '^pub\(crate\) mod serde;\s*$' "${facade}" >/dev/null || { echo "missing 'pub(crate) mod serde;'"; exit 1; }
"${rg_bin}" -n '^#\[cfg\(test\)\]\s*$' "${facade}" >/dev/null || { echo 'missing #[cfg(test)] in facade'; exit 1; }
"${rg_bin}" -n '^mod tests;\s*$' "${facade}" >/dev/null || { echo "missing 'mod tests;'"; exit 1; }

if "${rg_bin}" -n '^\s*(pub\s+)?fn\s+|^\s*impl\s+' "${facade}" >/dev/null; then
  echo 'facade must not define functions or impl blocks'
  "${rg_bin}" -n '^\s*(pub\s+)?fn\s+|^\s*impl\s+' "${facade}"
  exit 1
fi

if "${rg_bin}" -n 'Visitor|deserialize_|serialize_|is_false\(' "${facade}" >/dev/null; then
  echo 'serde helper logic leaked into facade'
  exit 1
fi

pub_use_count="$("${rg_bin}" -n '^pub use ' "${facade}" | wc -l | tr -d ' ')"
if [ "${pub_use_count}" -ne 2 ]; then
  echo "expected exactly 2 pub use lines in facade, got ${pub_use_count}"
  exit 1
fi

"${rg_bin}" -n 'MandateKind|OperationClass|MandateBuilder|Mandate|MandateContent' "${facade}" >/dev/null || {
  echo 'missing expected core re-export markers in facade'
  exit 1
}
"${rg_bin}" -n 'MANDATE_PAYLOAD_TYPE|MANDATE_REVOKED_PAYLOAD_TYPE|MANDATE_USED_PAYLOAD_TYPE' "${facade}" >/dev/null || {
  echo 'missing expected schema re-export markers in facade'
  exit 1
}

for f in "${core_file}" "${serde_file}" "${schema_file}"; do
  if [ ! -f "${f}" ]; then
    echo "missing expected file: ${f}"
    exit 1
  fi
  if "${rg_bin}" -n 'std::fs|tokio::fs|read_to_string|OpenOptions|File\b|PathBuf|env::|reqwest' "${f}" >/dev/null; then
    echo "unexpected IO/env/network markers in ${f}"
    "${rg_bin}" -n 'std::fs|tokio::fs|read_to_string|OpenOptions|File\b|PathBuf|env::|reqwest' "${f}"
    exit 1
  fi
done

"${rg_bin}" -n '^pub enum MandateKind' "${core_file}" >/dev/null
"${rg_bin}" -n '^pub enum OperationClass' "${core_file}" >/dev/null
"${rg_bin}" -n '^pub struct Mandate\b' "${core_file}" >/dev/null
"${rg_bin}" -n '^pub struct MandateBuilder\b' "${core_file}" >/dev/null

"${rg_bin}" -n '^\s*fn test_mandate_kind_serialization\(' "${tests_file}" >/dev/null
"${rg_bin}" -n '^\s*fn test_mandate_builder\(' "${tests_file}" >/dev/null
"${rg_bin}" -n '^\s*fn test_operation_class_serialization\(' "${tests_file}" >/dev/null

echo '== Mandate types Step3 diff allowlist =='
if [[ "${base_ref}" == *"wave18-mandate-types-step2-mechanical"* ]]; then
  allowlist_pattern='^docs/contributing/SPLIT-CHECKLIST-mandate-types-step3\.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step3\.md$|^scripts/ci/review-mandate-types-step3\.sh$'
else
  allowlist_pattern='^crates/assay-evidence/src/mandate/types\.rs$|^crates/assay-evidence/src/mandate/types/.*\.rs$|^docs/contributing/SPLIT-PLAN-wave18-mandate-types\.md$|^docs/contributing/SPLIT-CHECKLIST-mandate-types-step1\.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step1\.md$|^scripts/ci/review-mandate-types-step1\.sh$|^docs/contributing/SPLIT-CHECKLIST-mandate-types-step2\.md$|^docs/contributing/SPLIT-MOVE-MAP-mandate-types-step2\.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step2\.md$|^scripts/ci/review-mandate-types-step2\.sh$|^docs/contributing/SPLIT-CHECKLIST-mandate-types-step3\.md$|^docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step3\.md$|^scripts/ci/review-mandate-types-step3\.sh$'
fi

leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v "${allowlist_pattern}" || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in mandate-types Step3'
  exit 1
fi

echo 'Mandate types Step3 reviewer script: PASS'
