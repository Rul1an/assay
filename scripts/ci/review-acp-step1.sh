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
acp_root='crates/assay-adapter-acp'
api_root='crates/assay-adapter-api'
evidence_root='crates/assay-evidence'

echo '== ACP Step1 quality checks =='
cargo fmt --check
cargo clippy -p assay-adapter-api -p assay-adapter-acp --all-targets -- -D warnings
cargo test -p assay-adapter-acp

echo '== ACP Step1 freeze checks =='
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-adapter-acp/'; then
  echo 'tracked changes under crates/assay-adapter-acp/** are forbidden in Step1'
  exit 1
fi
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-adapter-api/'; then
  echo 'tracked changes under crates/assay-adapter-api/** are forbidden in Step1'
  exit 1
fi
if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^crates/assay-evidence/'; then
  echo 'tracked changes under crates/assay-evidence/** are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- "${acp_root}" | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-adapter-acp/** are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- "${api_root}" | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-adapter-api/** are forbidden in Step1'
  exit 1
fi
if git status --porcelain -- "${evidence_root}" | "${rg_bin}" '^\?\?'; then
  echo 'untracked files under crates/assay-evidence/** are forbidden in Step1'
  exit 1
fi

echo '== ACP Step1 diff allowlist =='
leaks="$({ git diff --name-only "${base_ref}...HEAD" | \
  "${rg_bin}" -v '^docs/contributing/SPLIT-PLAN-wave14-acp.md$|^docs/contributing/SPLIT-CHECKLIST-acp-step1.md$|^docs/contributing/SPLIT-REVIEW-PACK-acp-step1.md$|^scripts/ci/review-acp-step1.sh$' || true; })"
if [ -n "${leaks}" ]; then
  echo 'non-allowlisted files detected:'
  echo "${leaks}"
  exit 1
fi

if git diff --name-only "${base_ref}...HEAD" | "${rg_bin}" '^\.github/workflows/'; then
  echo 'workflow changes are forbidden in acp Step1'
  exit 1
fi

echo 'ACP Step1 reviewer script: PASS'
