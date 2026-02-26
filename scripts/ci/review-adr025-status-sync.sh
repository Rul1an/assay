#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-I2-CLOSURE-RELEASE-INTEGRATION.md"
  "docs/architecture/ADR-025-I3-OTEL-RELEASE-INTEGRATION.md"
  "docs/architecture/ADR-025-I2-STABILIZATION-POLICY.md"
  "docs/architecture/ADR-025-I3-STABILIZATION-POLICY.md"
  "docs/ROADMAP.md"
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

echo "[review] status markers present"
rg -n "Status Sync \(2026-02-25\)" docs/architecture/ADR-025-I2-CLOSURE-RELEASE-INTEGRATION.md >/dev/null || { echo "FAIL: missing I2 Step4 status sync"; exit 1; }
rg -n "Status Sync \(2026-02-25\)" docs/architecture/ADR-025-I3-OTEL-RELEASE-INTEGRATION.md >/dev/null || { echo "FAIL: missing I3 Step4 status sync"; exit 1; }
rg -n "Status Sync \(2026-02-25\)" docs/architecture/ADR-025-I2-STABILIZATION-POLICY.md >/dev/null || { echo "FAIL: missing I2 stabilization status sync"; exit 1; }
rg -n "Status Sync \(2026-02-25\)" docs/architecture/ADR-025-I3-STABILIZATION-POLICY.md >/dev/null || { echo "FAIL: missing I3 stabilization status sync"; exit 1; }

echo "[review] roadmap statuses updated"
rg -n "Audit Kit \(Manifest/Provenance\) \(ADR-025\).*Complete" docs/ROADMAP.md >/dev/null || { echo "FAIL: roadmap audit kit row not updated"; exit 1; }
rg -n "Soak Testing & Pass\^k \(ADR-025\).*Complete" docs/ROADMAP.md >/dev/null || { echo "FAIL: roadmap soak row not updated"; exit 1; }
rg -n "Closure Score & Completeness \(ADR-025\).*Complete" docs/ROADMAP.md >/dev/null || { echo "FAIL: roadmap closure row not updated"; exit 1; }

echo "[review] done"
