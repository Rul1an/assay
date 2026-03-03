#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "README.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-docs-positioning-agent-security.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && { echo "FAIL: no workflows"; exit 1; }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && { echo "FAIL: file not allowed: $f"; exit 1; }
done

rg -n "deterministic governance on the tool-call path|stateful sequence policies|tool-hopping" README.md >/dev/null || {
  echo "FAIL: README missing positioning markers"
  exit 1
}
rg -n "deterministic governance on the tool bus|multi-step leakage|tool-hopping|bounded claim" docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP missing positioning markers"
  exit 1
}

echo "[review] done"
