#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-011-Tool-Signing.md"
  "docs/architecture/ADR-012-Transparency-Log.md"
  "scripts/ci/review-adr011-012-boundary-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: boundary sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-011/012 boundary sync: $f"
    exit 1
  fi
done

ADR11="docs/architecture/ADR-011-Tool-Signing.md"
ADR12="docs/architecture/ADR-012-Transparency-Log.md"

echo "[review] boundary alignment"
rg -n "Enterprise pending: Sigstore keyless \+ transparency-log verification" "$ADR11" >/dev/null || {
  echo "FAIL: ADR-011 missing enterprise boundary note"
  exit 1
}
rg -n "enterprise extension" "$ADR11" >/dev/null || {
  echo "FAIL: ADR-011 missing enterprise extension wording"
  exit 1
}
rg -n "Rekor/Fulcio integration remains part of the enterprise advanced-signing surface" "$ADR12" >/dev/null || {
  echo "FAIL: ADR-012 missing enterprise boundary note"
  exit 1
}
if rg -n "public Rekor.*open source" "$ADR12" >/dev/null; then
  echo "FAIL: ADR-012 still claims public Rekor for open source"
  exit 1
fi

echo "[review] done"
