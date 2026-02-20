#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

base_ref="${BASE_REF:-origin/main}"
wf=".github/workflows/adr025-nightly-closure.yml"

allowlist=(
  ".github/workflows/adr025-nightly-closure.yml"
  "scripts/ci/review-adr025-i2-step3.sh"
  "docs/contributing/SPLIT-CHECKLIST-adr025-i2-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-i2-step3.md"
)

has_allowlisted_file() {
  local f="$1"
  for allowed in "${allowlist[@]}"; do
    if [[ "$f" == "$allowed" ]]; then
      return 0
    fi
  done
  return 1
}

check_diff_allowlist() {
  echo "[review] diff allowlist vs ${base_ref}"

  git rev-parse --verify "$base_ref" >/dev/null 2>&1 || {
    echo "FAIL: base ref '$base_ref' not found (run: git fetch origin)"
    exit 1
  }

  local changed
  changed="$(git diff --name-only "${base_ref}...HEAD")"

  if [[ -z "$changed" ]]; then
    echo "FAIL: no changes detected vs ${base_ref}"
    exit 1
  fi

  local leaked=0
  while IFS= read -r f; do
    [[ -z "$f" ]] && continue
    if ! has_allowlisted_file "$f"; then
      echo "FAIL: non-allowlisted file changed: $f"
      leaked=1
    fi
  done <<< "$changed"

  if [[ "$leaked" -ne 0 ]]; then
    exit 1
  fi
}

extract_top_level_event_keys() {
  local file="$1"
  awk '
    BEGIN {in_on=0}
    /^on:[[:space:]]*$/ {in_on=1; next}
    in_on && /^[^[:space:]]/ {in_on=0}
    in_on && /^  [A-Za-z_][A-Za-z0-9_]*:[[:space:]]*$/ {
      key=$1
      sub(":", "", key)
      print key
    }
  ' "$file"
}

check_sched_dispatch_only() {
  rg -n "^on:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing top-level on: block"; exit 1; }
  rg -n "^  schedule:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing schedule trigger"; exit 1; }
  rg -n "^  workflow_dispatch:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing workflow_dispatch trigger"; exit 1; }

  while IFS= read -r key; do
    [[ -z "$key" ]] && continue
    if [[ "$key" != "schedule" && "$key" != "workflow_dispatch" ]]; then
      echo "FAIL: non-allowed trigger under on:: $key"
      exit 1
    fi
  done < <(extract_top_level_event_keys "$wf")
}

check_no_pr_trigger() {
  if rg -n "pull_request|pull_request_target" "$wf" >/dev/null; then
    echo "FAIL: workflow must not include pull_request/pull_request_target"
    exit 1
  fi
}

check_informational_job_level() {
  if ! rg -n "^    continue-on-error:[[:space:]]*true[[:space:]]*$" "$wf" >/dev/null; then
    echo "FAIL: missing job-level continue-on-error: true"
    exit 1
  fi

  if rg -n "^[[:space:]]{6,}continue-on-error:[[:space:]]*true[[:space:]]*$" "$wf" >/dev/null; then
    echo "FAIL: step-level continue-on-error found; require job-level only"
    exit 1
  fi
}

check_sha_pins_strict() {
  local bad=0
  while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    local token
    token="$(sed -E 's/^[[:space:]]*uses:[[:space:]]+([^[:space:]#]+).*/\1/' <<< "$line")"

    if [[ "$token" =~ ^\./ ]]; then
      continue
    fi

    if [[ ! "$token" =~ ^[^@]+@[0-9a-f]{40}$ ]]; then
      echo "FAIL: non-SHA remote uses ref: $line"
      bad=1
    fi
  done < <(rg "^[[:space:]]*uses:[[:space:]]+[^[:space:]#]+" "$wf" || true)

  if [[ "$bad" -ne 0 ]]; then
    exit 1
  fi
}

check_permissions_minimal() {
  rg -n "^permissions:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing permissions block"; exit 1; }
  rg -n "^  contents:[[:space:]]*read[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing contents: read"; exit 1; }
  rg -n "^  actions:[[:space:]]*write[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: missing actions: write"; exit 1; }

  if rg -n "id-token:[[:space:]]*write" "$wf" >/dev/null; then
    echo "FAIL: must not request id-token: write"
    exit 1
  fi

  if rg -n "permissions:[[:space:]]*(write-all|read-all)" "$wf" >/dev/null; then
    echo "FAIL: broad permissions (*-all) are not allowed"
    exit 1
  fi
}

check_artifact_contract() {
  rg -n "name:[[:space:]]*adr025-closure-report" "$wf" >/dev/null || { echo "FAIL: closure artifact name mismatch"; exit 1; }
  rg -n "retention-days:[[:space:]]*14" "$wf" >/dev/null || { echo "FAIL: closure artifact retention mismatch"; exit 1; }
  rg -n "adr025-closure-evaluate\.sh" "$wf" >/dev/null || { echo "FAIL: closure evaluator invocation missing"; exit 1; }
}

test -f "$wf" || { echo "FAIL: missing $wf"; exit 1; }

check_diff_allowlist
check_no_pr_trigger
check_sched_dispatch_only
check_informational_job_level
check_sha_pins_strict
check_permissions_minimal
check_artifact_contract

echo "[review] done"
