#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/adrs.md"
  "scripts/ci/review-adrs-index-open-core-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR index open-core sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR index open-core sync: $f"
    exit 1
  fi
done

echo "[review] required ADR entries"
rg -n '\[ADR-021\].*Local Pack Discovery' docs/architecture/adrs.md >/dev/null || { echo "FAIL: missing ADR-021 index entry"; exit 1; }
rg -n '\[ADR-022\].*SOC2 Baseline Pack' docs/architecture/adrs.md >/dev/null || { echo "FAIL: missing ADR-022 index entry"; exit 1; }
rg -n '\[ADR-023\].*CICD Starter Pack' docs/architecture/adrs.md >/dev/null || { echo "FAIL: missing ADR-023 index entry"; exit 1; }
rg -n '\[ADR-025\].*Evidence-as-a-Product' docs/architecture/adrs.md >/dev/null || { echo "FAIL: missing ADR-025 index entry"; exit 1; }

echo "[review] open-core boundary note retained"
rg -n 'Sigstore keyless deferred to enterprise' docs/architecture/adrs.md >/dev/null || { echo "FAIL: missing ADR-011 enterprise boundary note"; exit 1; }

echo "[review] done"
