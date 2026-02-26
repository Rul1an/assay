#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-011-Tool-Signing.md"
  "docs/architecture/adrs.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-open-core-boundary-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: open-core boundary sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in open-core boundary sync: $f"
    exit 1
  fi
done

echo "[review] open-core boundary markers"
rg -n "Enterprise pending: Sigstore keyless" docs/architecture/ADR-011-Tool-Signing.md >/dev/null || {
  echo "FAIL: ADR-011 must explicitly mark keyless as enterprise pending"
  exit 1
}
rg -n "Sigstore keyless deferred to enterprise" docs/architecture/adrs.md >/dev/null || {
  echo "FAIL: adrs.md must reflect keyless enterprise boundary"
  exit 1
}

echo "[review] roadmap sync markers"
rg -n "Sigstore Keyless \(Enterprise\).*Pending" docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap must keep Sigstore keyless as enterprise pending"
  exit 1
}
rg -n "Pack Registry.*implemented in PR #287" docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap must mark ADR-021 pack registry implemented"
  exit 1
}

echo "[review] done"
