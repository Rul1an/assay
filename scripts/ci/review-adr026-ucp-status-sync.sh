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
  "scripts/ci/review-adr026-ucp-status-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 UCP status sync must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 UCP status sync: $f"
    exit 1
  fi
done

echo "[review] ADR-026 UCP status markers"
rg -n 'ADR-026.*Accepted' docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: adrs index missing accepted ADR-026 status"
  exit 1
}
rg -n '^\- \[x\] \*\*UCP adapter\*\*' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing completed UCP adapter marker"
  exit 1
}
rg -n 'assay-adapter-ucp.*merged in open core' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing merged UCP status line"
  exit 1
}
rg -n 'ACP \+ A2A \+ UCP adapter rollout' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 status line missing UCP rollout marker"
  exit 1
}
rg -n '^\- `assay-adapter-ucp` is implemented with strict/lenient conversion, fixtures, and conformance tests$' docs/architecture/ADR-026-Protocol-Adapters.md >/dev/null || {
  echo "FAIL: ADR-026 implementation status missing UCP line"
  exit 1
}

echo "[review] done"
