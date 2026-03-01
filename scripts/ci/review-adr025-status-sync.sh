#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-Evidence-as-a-Product.md"
  "docs/architecture/adrs.md"
  "scripts/ci/review-adr025-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-025 status sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-025 status sync: $f"
    exit 1
  fi
done

echo "[review] status markers"
rg -n '^Accepted \(March 2026; I1/I2/I3 rollout slices implemented and closed-loop on `main`\)$' \
  docs/architecture/ADR-025-Evidence-as-a-Product.md >/dev/null || {
  echo "FAIL: ADR-025 accepted status missing"
  exit 1
}

rg -n '\| \[ADR-025\]\(\./ADR-025-Evidence-as-a-Product.md\) \| Evidence-as-a-Product \| Accepted \| \*\*P1/P2\*\* \|' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: adrs index missing accepted ADR-025 row"
  exit 1
}

rg -n 'ADR-025.*Accepted.*I1/I2/I3 slices merged on `main`; formal accept complete' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: priorities table missing ADR-025 accepted note"
  exit 1
}

echo "[review] done"
