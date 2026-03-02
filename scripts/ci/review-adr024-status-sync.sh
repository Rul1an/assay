#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/adrs.md"
  "scripts/ci/review-adr024-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-024 status sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-024 status sync: $f"
    exit 1
  fi
done

echo "[review] status markers"
rg -n '^Superseded \(February 2026, by ADR-025 Reliability Surface / I1 soak rollout\)$' \
  docs/architecture/ADR-024-Sim-Engine-Hardening.md >/dev/null || {
  echo "FAIL: ADR-024 source ADR is not marked superseded"
  exit 1
}

rg -n '\| \[ADR-024\]\(\./ADR-024-Sim-Engine-Hardening.md\) \| Sim Engine Hardening \(Limits \+ Time Budget\) \| Superseded \| \*\*P2\*\* \|' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-024 adrs row not marked superseded"
  exit 1
}

echo "[review] done"
