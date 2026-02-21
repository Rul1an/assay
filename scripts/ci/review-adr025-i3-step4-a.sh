#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-I3-OTEL-RELEASE-INTEGRATION.md"
  "schemas/otel_release_policy_v1.json"
  "scripts/ci/review-adr025-i3-step4-a.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I3 Step4A must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I3 Step4A: $f"
    exit 1
  fi
done

echo "[review] policy JSON parses"
python3 - <<'PY'
import json
json.load(open("schemas/otel_release_policy_v1.json", "r", encoding="utf-8"))
print("otel_release_policy_v1.json: ok")
PY

echo "[review] done"
