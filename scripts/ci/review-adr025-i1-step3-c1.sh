#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

WF=".github/workflows/adr025-nightly-soak.yml"

echo "[review] checking $WF exists"
test -f "$WF"

echo "[review] ensure no pull_request trigger"
if rg -n "pull_request" "$WF" >/dev/null; then
  echo "FAIL: workflow must not include pull_request trigger"
  exit 1
fi

echo "[review] ensure schedule + workflow_dispatch exist"
rg -n "schedule:" "$WF" >/dev/null || { echo "FAIL: missing schedule trigger"; exit 1; }
rg -n "workflow_dispatch:" "$WF" >/dev/null || { echo "FAIL: missing workflow_dispatch trigger"; exit 1; }

echo "[review] ensure continue-on-error true (informational lane)"
rg -n "continue-on-error:\s*true" "$WF" >/dev/null || {
  echo "FAIL: soak job must be continue-on-error: true"
  exit 1
}

echo "[review] ensure minimal permissions are explicitly set"
rg -n "^permissions:" "$WF" >/dev/null || { echo "FAIL: missing permissions block"; exit 1; }
rg -n "contents:\s*read" "$WF" >/dev/null || { echo "FAIL: missing contents: read"; exit 1; }
rg -n "actions:\s*write" "$WF" >/dev/null || { echo "FAIL: missing actions: write (needed for artifacts)"; exit 1; }

echo "[review] done"
