#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/adrs.md"
  "docs/ROADMAP.md"
  "docs/DX-ROADMAP.md"
  "scripts/ci/review-p3-adr-roadmap-consistency.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: P3 consistency slice must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in P3 consistency slice: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] marker checks"
rg -n 'ADR-027|ADR-028|ADR-029|ADR-030' docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR index missing ADR-027..030 entries"
  exit 1
}
rg -n 'Governance core status \(2026-03-04\)' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP missing governance core status sync"
  exit 1
}
rg -n 'Status sync \(2026-03-04\).*B4 DX polish' docs/DX-ROADMAP.md >/dev/null || {
  echo "FAIL: DX roadmap missing B4 status sync"
  exit 1
}

echo "[review] done"
