#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/adrs.md"
  "scripts/ci/review-adr013-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-013 status sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-013 status sync: $f"
    exit 1
  fi
done

echo "[review] status markers"
rg -n '^Accepted \(January 2026\)$' docs/architecture/ADR-013-EU-AI-Act-Pack.md >/dev/null || {
  echo "FAIL: ADR-013 source ADR is not accepted"
  exit 1
}

rg -n '\| \[ADR-013\]\(\./ADR-013-EU-AI-Act-Pack.md\) \| EU AI Act Compliance Pack \| Accepted \| \*\*P2\*\* \|' docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-013 index row not marked accepted"
  exit 1
}

rg -n '\| \*\*P2\*\* \| \[ADR-013\]\(\./ADR-013-EU-AI-Act-Pack.md\) \| Accepted \| Article 12 mapping, `--pack` flag \|' docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-013 priorities row not marked accepted"
  exit 1
}

echo "[review] done"
