#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-026-PARSER-HARDENING-BOUNDARY.md"
  "scripts/ci/review-adr026-stab-e4-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E4A must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E4A: $f"
    exit 1
  fi
done

DOC="docs/architecture/ADR-026-PARSER-HARDENING-BOUNDARY.md"

echo "[review] parser hardening markers"
rg -n "deeply nested JSON|deep nesting" "$DOC" >/dev/null || {
  echo "FAIL: threat model must mention deep nesting"
  exit 1
}
rg -n "large arrays|array length" "$DOC" >/dev/null || {
  echo "FAIL: threat model must mention large arrays"
  exit 1
}
rg -n "invalid UTF-8" "$DOC" >/dev/null || {
  echo "FAIL: threat model must mention invalid UTF-8"
  exit 1
}
rg -n "max_payload_bytes|max_json_depth|max_array_length" "$DOC" >/dev/null || {
  echo "FAIL: hard caps contract missing"
  exit 1
}
rg -n "measurement/contract failure|measurement/contract failures" "$DOC" >/dev/null || {
  echo "FAIL: measurement failure contract missing"
  exit 1
}

echo "[review] done"
