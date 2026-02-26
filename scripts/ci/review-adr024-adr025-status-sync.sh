#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-024-Sim-Engine-Hardening.md"
  "docs/architecture/ADR-025-Evidence-as-a-Product.md"
  "scripts/ci/review-adr024-adr025-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-024/025 status sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-024/025 status sync: $f"
    exit 1
  fi
done

echo "[review] ADR status markers"
rg -n '^Superseded \(February 2026, by ADR-025 Reliability Surface / I1 soak rollout\)$' docs/architecture/ADR-024-Sim-Engine-Hardening.md >/dev/null || {
  echo "FAIL: ADR-024 status must be marked superseded by ADR-025"
  exit 1
}
rg -n '^## Status Sync \(2026-02-26\)$' docs/architecture/ADR-024-Sim-Engine-Hardening.md >/dev/null || {
  echo "FAIL: ADR-024 missing status sync block"
  exit 1
}
rg -n '^Proposed \(Feb 2026; rollout slices I1/I2/I3 implemented on `main`\)$' docs/architecture/ADR-025-Evidence-as-a-Product.md >/dev/null || {
  echo "FAIL: ADR-025 status line must reflect implemented rollout slices"
  exit 1
}
rg -n '^## Status Sync \(2026-02-26\)$' docs/architecture/ADR-025-Evidence-as-a-Product.md >/dev/null || {
  echo "FAIL: ADR-025 missing status sync block"
  exit 1
}

echo "[review] done"
