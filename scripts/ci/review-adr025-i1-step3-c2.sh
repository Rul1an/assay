#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

WF=".github/workflows/adr025-nightly-readiness.yml"

echo "[review] checking $WF exists"
test -f "$WF"

echo "[review] no pull_request trigger"
if rg -n "pull_request" "$WF" >/dev/null; then
  echo "FAIL: workflow must not include pull_request trigger"
  exit 1
fi

echo "[review] schedule + workflow_dispatch required"
rg -n "schedule:" "$WF" >/dev/null || { echo "FAIL: missing schedule"; exit 1; }
rg -n "workflow_dispatch:" "$WF" >/dev/null || { echo "FAIL: missing workflow_dispatch"; exit 1; }

echo "[review] continue-on-error must be true"
rg -n "continue-on-error:\s*true" "$WF" >/dev/null || { echo "FAIL: missing continue-on-error: true"; exit 1; }

echo "[review] actions must be SHA-pinned"
if rg -n 'uses:\s+\S+@(v[0-9]+|stable|main|master|nightly)\b' "$WF" >/dev/null; then
  echo "FAIL: actions refs must be pinned to commit SHA (no @vN/@stable/@main/@master/@nightly)"
  exit 1
fi
if ! rg -n 'uses:\s+\S+@[0-9a-f]{40}\b' "$WF" >/dev/null; then
  echo "FAIL: expected at least one SHA-pinned uses: ...@<40-hex>"
  exit 1
fi

echo "[review] artifact name + retention contract"
rg -n "name:\s*adr025-nightly-readiness" "$WF" >/dev/null || { echo "FAIL: missing artifact name adr025-nightly-readiness"; exit 1; }
rg -n "retention-days:\s*14" "$WF" >/dev/null || { echo "FAIL: missing retention-days: 14"; exit 1; }

echo "[review] done"
