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
model_dir='crates/assay-core/src/model'

echo '== Model Step1 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib model::tests::test_string_input_deserialize -- --exact
cargo test -p assay-core --lib model::tests::test_legacy_list_expected -- --exact
cargo test -p assay-core --lib model::tests::test_validate_ref_in_v1 -- --exact

echo '== Model Step1 freeze checks =='
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-core/src/model\.rs$|^crates/assay-core/src/model/'; then
  echo 'tracked changes under crates/assay-core/src/model.rs or crates/assay-core/src/model/** are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- "${model_dir}" | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/model/** are forbidden in Step1'
  exit 1
fi

echo '== Model Step1 diff allowlist =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-PLAN-wave13-model.md$|^docs/contributing/SPLIT-CHECKLIST-model-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-model-step1.md$|^scripts/ci/review-model-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in model Step1'
  exit 1
fi

echo 'Model Step1 reviewer script: PASS'
