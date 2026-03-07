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
agentic_root='crates/assay-core/src/agentic'

echo '== Agentic Step1 quality checks =='
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core --lib agentic::tests::test_deduplication -- --exact
cargo test -p assay-core --lib agentic::tests::test_detect_policy_shape -- --exact
cargo test -p assay-core --lib agentic::tests::test_tool_poisoning_action_uses_assay_config_not_policy -- --exact

echo '== Agentic Step1 freeze checks =='
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-core/src/agentic/'; then
  echo 'tracked changes under crates/assay-core/src/agentic are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- "${agentic_root}" | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-core/src/agentic are forbidden in Step1'
  exit 1
fi

echo '== Agentic Step1 diff allowlist =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-PLAN-wave12-agentic.md$|^docs/contributing/SPLIT-CHECKLIST-agentic-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-agentic-step1.md$|^scripts/ci/review-agentic-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in agentic Step1'
  exit 1
fi

echo 'Agentic Step1 reviewer script: PASS'
