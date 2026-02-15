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
wf_file=".github/workflows/wave6-nightly-safety.yml"

check_has_match() {
  local pattern="$1"
  local file="$2"
  if [ ! -f "$file" ]; then
    echo "missing expected file: ${file}"
    exit 1
  fi
  if ! "$rg_bin" -n -- "$pattern" "$file" >/dev/null; then
    echo "missing expected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

check_no_match() {
  local pattern="$1"
  local file="$2"
  if [ ! -f "$file" ]; then
    echo "missing expected file: ${file}"
    exit 1
  fi
  if "$rg_bin" -n -- "$pattern" "$file" >/dev/null; then
    echo "unexpected pattern in ${file}: ${pattern}"
    exit 1
  fi
}

echo "== Wave6 Step4 policy + workflow checks =="
check_has_match '^name:[[:space:]]+Wave6 Nightly Safety' "$wf_file"
check_has_match '^on:' "$wf_file"
check_has_match 'schedule:' "$wf_file"
check_has_match 'workflow_dispatch:' "$wf_file"
check_has_match 'miri-registry-smoke:' "$wf_file"
check_has_match 'proptest-cli-smoke:' "$wf_file"
check_has_match 'nightly-summary:' "$wf_file"
check_has_match 'continue-on-error:[[:space:]]+true' "$wf_file"
check_has_match '^permissions:[[:space:]]*\{\}' "$wf_file"
check_has_match 'actions:[[:space:]]+read' "$wf_file"
check_no_match 'id-token:[[:space:]]+write' "$wf_file"

echo "== Wave6 Step4 artifact + classifier contract =="
check_has_match 'Generate nightly status artifact \(Option A: API aggregator\)' "$wf_file"
check_has_match 'api_url=\"https://api\.github\.com/repos/\$\{GITHUB_REPOSITORY\}/actions/runs/\$\{GITHUB_RUN_ID\}/jobs\?per_page=100\"' "$wf_file"
check_has_match 'name:[[:space:]]+nightly-status' "$wf_file"
check_has_match 'path:[[:space:]]+nightly_status\.json' "$wf_file"
check_has_match 'retention-days:[[:space:]]+14' "$wf_file"
check_has_match 'schema_version:[[:space:]]+1' "$wf_file"
check_has_match 'classifier_version:[[:space:]]+1' "$wf_file"
check_has_match '\$attempt[[:space:]]*>[[:space:]]*1' "$wf_file"
check_has_match '\(\$c == \"cancelled\" or \$c == \"timed_out\"\)' "$wf_file"
check_has_match '\$c == \"failure\"' "$wf_file"
check_has_match 'workflow_conclusion' "$wf_file"
check_has_match 'workflow_category' "$wf_file"

echo "== Wave6 Step4 diff allowlist =="
leaks="$($rg_bin -v \
  '^\.github/workflows/wave6-nightly-safety\.yml$|^docs/contributing/SPLIT-(INVENTORY|CHECKLIST|REVIEW-PACK)-wave6-step4-nightly-promotion\.md$|^scripts/ci/review-wave6-step4-ci\.sh$|^docs/architecture/PLAN-split-refactor-2026q1\.md$' \
  < <(git diff --name-only "${base_ref}...HEAD") || true)"
if [ -n "${leaks}" ]; then
  echo "non-allowlisted files detected:"
  echo "${leaks}"
  exit 1
fi

echo "Wave6 Step4 reviewer script: PASS"
