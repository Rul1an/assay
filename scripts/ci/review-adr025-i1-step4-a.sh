#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

echo "[review] BASE_REF=$BASE_REF"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-SOAK-ENFORCEMENT-POLICY.md"
  "schemas/soak_readiness_policy_v1.json"
  "scripts/ci/review-adr025-i1-step4-a.sh"
)

echo "[review] diff allowlist"
changed="$(git diff --name-only "$BASE_REF"...HEAD)"

# block any workflow changes in A-slice
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step4A must not change workflows ($f)"
    exit 1
  fi
done <<< "$changed"

# enforce allowlist only
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    if [[ "$f" == "$a" ]]; then ok="true"; break; fi
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Step4A: $f"
    exit 1
  fi
done <<< "$changed"

echo "[review] ensure policy JSON parses"
python3 - <<'PY'
import json
json.load(open("schemas/soak_readiness_policy_v1.json","r",encoding="utf-8"))
print("policy json: ok")
PY

echo "[review] done"
