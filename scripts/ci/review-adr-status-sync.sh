#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-Evidence-as-a-Product.md"
  "docs/architecture/ADR-026-Protocol-Adapters.md"
  "docs/architecture/adrs.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: status sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR status sync: $f"
    exit 1
  fi
done

echo "[review] ADR statuses"
rg -n '^Accepted \(Feb 2026; rollout slices I1/I2/I3 implemented on `main`\)$' \
  docs/architecture/ADR-025-Evidence-as-a-Product.md >/dev/null || {
  echo "FAIL: ADR-025 status not synced"
  exit 1
}

rg -n '^Accepted \(February 2026; ACP and A2A open-core adapters implemented on `main`\)$' \
  docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 status not synced"
  exit 1
}

echo "[review] index and roadmap references"
rg -n '\| \[ADR-013\]\(\./ADR-013-EU-AI-Act-Pack\.md\) \| EU AI Act Compliance Pack \| Accepted \| \*\*P2\*\* \|' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-013 index row not synced"
  exit 1
}

rg -n '\| \[ADR-025\]\(\./ADR-025-Evidence-as-a-Product\.md\) \| Evidence-as-a-Product \| Accepted \| \*\*P1/P2\*\* \|' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-025 index row not synced"
  exit 1
}

rg -n '\| \[ADR-026\]\(\./ADR-026-Protocol-Adapters\.md\) \| Protocol Adapters \| Accepted \| \*\*P1\*\* \|' \
  docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: ADR-026 index row not synced"
  exit 1
}

rg -n '\| \*\*P1\*\* \| Protocol Adapters \(ADR-026\) \| Medium \| High \| ✅ Complete \(ACP \+ A2A MVP\) \|' \
  docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP missing ADR-026 completion row"
  exit 1
}

echo "[review] done"
