#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

base_ref="${BASE_REF:-origin/main}"

WFS=(
  ".github/workflows/adr025-nightly-soak.yml"
  ".github/workflows/adr025-nightly-readiness.yml"
)

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr025-step3-c3-rollout.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-step3-c3-rollout.md"
  "scripts/ci/review-adr025-i1-step3-c3.sh"
)

has_allowlisted_file() {
  local f="$1"
  for allowed in "${ALLOWLIST[@]}"; do
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
  changed=$(git diff --name-only "${base_ref}...HEAD")

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
  local wf="$1"
  awk '
    BEGIN {in_on=0}
    /^on:[[:space:]]*$/ {in_on=1; next}
    in_on && /^[^[:space:]]/ {in_on=0}
    in_on && /^  [A-Za-z_][A-Za-z0-9_]*:[[:space:]]*$/ {
      key=$1
      sub(":", "", key)
      print key
    }
  ' "$wf"
}

check_no_pr_trigger() {
  local wf="$1"
  if rg -n "pull_request|pull_request_target" "$wf" >/dev/null; then
    echo "FAIL: $wf must not include pull_request/pull_request_target trigger"
    exit 1
  fi
}

check_sched_dispatch_only() {
  local wf="$1"

  if ! rg -n "^on:[[:space:]]*$" "$wf" >/dev/null; then
    echo "FAIL: $wf missing top-level on: block"
    exit 1
  fi

  rg -n "^  schedule:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: $wf missing schedule trigger"; exit 1; }
  rg -n "^  workflow_dispatch:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: $wf missing workflow_dispatch trigger"; exit 1; }

  while IFS= read -r key; do
    [[ -z "$key" ]] && continue
    if [[ "$key" != "schedule" && "$key" != "workflow_dispatch" ]]; then
      echo "FAIL: $wf has non-allowed trigger under on:: $key"
      exit 1
    fi
  done < <(extract_top_level_event_keys "$wf")
}

check_informational_job_level() {
  local wf="$1"

  # Require job-level continue-on-error (4-space indent), not just step-level.
  if ! rg -n "^    continue-on-error:[[:space:]]*true[[:space:]]*$" "$wf" >/dev/null; then
    echo "FAIL: $wf missing job-level continue-on-error: true"
    exit 1
  fi

  # Guard against step-level attempts to satisfy the check.
  if rg -n "^[[:space:]]{6,}continue-on-error:[[:space:]]*true[[:space:]]*$" "$wf" >/dev/null; then
    echo "FAIL: $wf has step-level continue-on-error; require job-level only"
    exit 1
  fi
}

check_sha_pins_strict() {
  local wf="$1"
  local bad=0

  while IFS= read -r line; do
    [[ -z "$line" ]] && continue

    # Extract token after `uses:`
    local token
    token="$(sed -E 's/^[[:space:]]*uses:[[:space:]]+([^[:space:]#]+).*/\1/' <<< "$line")"

    # Local actions are allowed without @ref.
    if [[ "$token" =~ ^\./ ]]; then
      continue
    fi

    # Remote actions must be pinned to a 40-hex SHA.
    if [[ ! "$token" =~ ^[^@]+@[0-9a-f]{40}$ ]]; then
      echo "FAIL: $wf has non-SHA remote uses ref: $line"
      bad=1
    fi
  done < <(rg "^[[:space:]]*uses:[[:space:]]+[^[:space:]#]+" "$wf" || true)

  if [[ "$bad" -ne 0 ]]; then
    exit 1
  fi
}

check_permissions_minimal() {
  local wf="$1"

  rg -n "^permissions:[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: $wf missing permissions block"; exit 1; }

  if rg -n "permissions:[[:space:]]*(write-all|read-all)" "$wf" >/dev/null; then
    echo "FAIL: $wf uses broad permissions (*-all)"
    exit 1
  fi

  if rg -n "id-token:[[:space:]]*write" "$wf" >/dev/null; then
    echo "FAIL: $wf must not request id-token: write"
    exit 1
  fi

  # Require baseline permissions for ADR-025 workflows.
  rg -n "^  contents:[[:space:]]*read[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: $wf missing permissions contents: read"; exit 1; }
  rg -n "^  actions:[[:space:]]*write[[:space:]]*$" "$wf" >/dev/null || { echo "FAIL: $wf missing permissions actions: write"; exit 1; }

  # Allowlist keys to avoid over-broad permissions while leaving controlled extension room.
  local invalid
  invalid="$(awk '
    BEGIN {in_p=0}
    /^permissions:[[:space:]]*$/ {in_p=1; next}
    in_p && /^[^[:space:]]/ {in_p=0}
    in_p && /^  [A-Za-z-]+:[[:space:]]*[A-Za-z-]+[[:space:]]*$/ {
      key=$1; sub(":", "", key)
      val=$2
      if (key != "contents" && key != "actions" && key != "pull-requests" && key != "security-events") {
        print "key:" key
      }
      if (key == "contents" && val != "read") {
        print "contents:" val
      }
      if (key == "actions" && val != "write") {
        print "actions:" val
      }
      if (key == "pull-requests" && val != "read" && val != "write") {
        print "pull-requests:" val
      }
      if (key == "security-events" && val != "read" && val != "write") {
        print "security-events:" val
      }
    }
  ' "$wf")"

  if [[ -n "$invalid" ]]; then
    echo "FAIL: $wf has invalid permission policy entries: $(tr '\n' ' ' <<< "$invalid")"
    exit 1
  fi
}

echo "[review] workflows exist"
for wf in "${WFS[@]}"; do
  test -f "$wf" || { echo "FAIL: missing $wf"; exit 1; }
done

check_diff_allowlist

echo "[review] policy checks"
for wf in "${WFS[@]}"; do
  check_no_pr_trigger "$wf"
  check_sched_dispatch_only "$wf"
  check_informational_job_level "$wf"
  check_sha_pins_strict "$wf"
  check_permissions_minimal "$wf"
done

echo "[review] artifact contract checks"
rg -n "name:[[:space:]]*adr025-soak-report" .github/workflows/adr025-nightly-soak.yml >/dev/null || { echo "FAIL: soak artifact name mismatch"; exit 1; }
rg -n "retention-days:[[:space:]]*14" .github/workflows/adr025-nightly-soak.yml >/dev/null || { echo "FAIL: soak retention mismatch"; exit 1; }

rg -n "name:[[:space:]]*adr025-nightly-readiness" .github/workflows/adr025-nightly-readiness.yml >/dev/null || { echo "FAIL: readiness artifact name mismatch"; exit 1; }
rg -n "retention-days:[[:space:]]*14" .github/workflows/adr025-nightly-readiness.yml >/dev/null || { echo "FAIL: readiness retention mismatch"; exit 1; }

echo "[review] done"
