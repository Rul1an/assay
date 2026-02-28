#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/adrs.md"
  "docs/ROADMAP.md"
  "docs/architecture/ADR-026-Protocol-Adapters.md"
  "scripts/ci/review-adr026-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 status sync must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 status sync: $f"
    exit 1
  fi
done

echo "[review] ADR-026 status markers"
rg -n 'ADR-026.*Accepted' docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: adrs index missing accepted ADR-026 status"
  exit 1
}
rg -n '^\- \[x\] \*\*Adapter trait\*\*' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing completed adapter trait marker"
  exit 1
}
rg -n '^\- \[x\] \*\*ACP adapter\*\*' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing completed ACP adapter marker"
  exit 1
}
rg -n '^\- \[x\] \*\*A2A adapter\*\*' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing completed A2A adapter marker"
  exit 1
}
rg -n '^Accepted .*E0-E4 stabilization' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 status line missing accepted implementation status"
  exit 1
}

echo "[review] done"
