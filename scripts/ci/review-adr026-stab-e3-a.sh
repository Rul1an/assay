#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-stab-e2-b-host-writer}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-026-CANONICALIZATION-HASH-BOUNDARY.md"
  "scripts/ci/review-adr026-stab-e3-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E3A must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E3A: $f"
    exit 1
  fi
done

echo "[review] contract markers"
DOC="docs/architecture/ADR-026-CANONICALIZATION-HASH-BOUNDARY.md"
rg -n '^# ADR-026 Canonicalization and Hash Boundary \(E3A\)$' "$DOC" >/dev/null || {
  echo "FAIL: missing E3A title"
  exit 1
}
rg -n '^## Canonical event payload contract \(v1\)$' "$DOC" >/dev/null || {
  echo "FAIL: missing canonical payload contract"
  exit 1
}
rg -n '^## Raw payload hash boundary$' "$DOC" >/dev/null || {
  echo "FAIL: missing raw payload boundary"
  exit 1
}
rg -n 'same canonical event payload digest' "$DOC" >/dev/null || {
  echo "FAIL: missing key-order independence rule"
  exit 1
}
rg -n 'exact raw payload bytes remain distinguishable' "$DOC" >/dev/null || {
  echo "FAIL: missing raw-byte distinguishability rule"
  exit 1
}
rg -n 'canonical_json_bytes' "$DOC" >/dev/null || {
  echo "FAIL: missing shared utility contract"
  exit 1
}

echo "[review] done"
