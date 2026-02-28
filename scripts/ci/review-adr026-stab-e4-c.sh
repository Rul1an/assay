#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-stab-e4-b-parser-hardening}"
BASE_REF_IMPL="${BASE_REF_IMPL:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null
git rev-parse --verify "$BASE_REF_IMPL" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr026-stab-e4.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr026-stab-e4.md"
  "scripts/ci/review-adr026-stab-e4-c.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: E4C must not touch workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in E4C: $f"
    exit 1
  fi
done

echo "[review] closure artifacts mention parser hardening"
rg -n 'max_json_depth|max_array_length|Invalid UTF-8|Lenient mode does not bypass' docs/contributing/SPLIT-CHECKLIST-adr026-stab-e4.md >/dev/null || {
  echo "FAIL: E4 checklist is missing parser hardening invariants"
  exit 1
}
rg -n 'review-adr026-stab-e4-b.sh|shape.rs|ADR-026-PARSER-HARDENING-BOUNDARY.md' docs/contributing/SPLIT-REVIEW-PACK-adr026-stab-e4.md >/dev/null || {
  echo "FAIL: E4 review pack is missing core references"
  exit 1
}

echo "[review] re-run E4B implementation gate vs $BASE_REF_IMPL"
BASE_REF="$BASE_REF_IMPL" bash scripts/ci/review-adr026-stab-e4-b.sh

echo "[review] done"
