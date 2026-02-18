#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

WFS=(
  ".github/workflows/adr025-nightly-soak.yml"
  ".github/workflows/adr025-nightly-readiness.yml"
)

echo "[review] workflows exist"
for wf in "${WFS[@]}"; do
  test -f "$wf" || { echo "FAIL: missing $wf"; exit 1; }
done

check_no_pr_trigger() {
  local wf="$1"
  if rg -n "pull_request" "$wf" >/dev/null; then
    echo "FAIL: $wf must not include pull_request trigger"
    exit 1
  fi
}

check_sched_dispatch() {
  local wf="$1"
  rg -n "schedule:" "$wf" >/dev/null || { echo "FAIL: $wf missing schedule"; exit 1; }
  rg -n "workflow_dispatch:" "$wf" >/dev/null || { echo "FAIL: $wf missing workflow_dispatch"; exit 1; }
}

check_informational() {
  local wf="$1"
  rg -n "continue-on-error:\s*true" "$wf" >/dev/null || { echo "FAIL: $wf must be continue-on-error: true"; exit 1; }
}

check_sha_pins() {
  local wf="$1"
  if rg -n 'uses:\s+\S+@(v[0-9]+|stable|main|master|nightly)\b' "$wf" >/dev/null; then
    echo "FAIL: $wf uses non-SHA action ref"
    exit 1
  fi
  if ! rg -n 'uses:\s+\S+@[0-9a-f]{40}\b' "$wf" >/dev/null; then
    echo "FAIL: $wf expected SHA-pinned action refs"
    exit 1
  fi
}

check_permissions_minimal() {
  local wf="$1"
  rg -n "^permissions:" "$wf" >/dev/null || { echo "FAIL: $wf missing permissions block"; exit 1; }
  if rg -n "id-token:\s*write" "$wf" >/dev/null; then
    echo "FAIL: $wf must not request id-token: write"
    exit 1
  fi
}

echo "[review] policy checks"
for wf in "${WFS[@]}"; do
  check_no_pr_trigger "$wf"
  check_sched_dispatch "$wf"
  check_informational "$wf"
  check_sha_pins "$wf"
  check_permissions_minimal "$wf"
done

echo "[review] artifact contract checks"
rg -n "name:\s*adr025-soak-report" .github/workflows/adr025-nightly-soak.yml >/dev/null || { echo "FAIL: soak artifact name mismatch"; exit 1; }
rg -n "retention-days:\s*14" .github/workflows/adr025-nightly-soak.yml >/dev/null || { echo "FAIL: soak retention mismatch"; exit 1; }

rg -n "name:\s*adr025-nightly-readiness" .github/workflows/adr025-nightly-readiness.yml >/dev/null || { echo "FAIL: readiness artifact name mismatch"; exit 1; }
rg -n "retention-days:\s*14" .github/workflows/adr025-nightly-readiness.yml >/dev/null || { echo "FAIL: readiness retention mismatch"; exit 1; }

echo "[review] done"
