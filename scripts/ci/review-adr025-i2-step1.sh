#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

echo "[review] BASE_REF=$BASE_REF"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-ADR-025-I2-audit-kit-closure-2026q2.md"
  "schemas/closure_report_v1.schema.json"
  "scripts/ci/review-adr025-i2-step1.sh"
)

echo "[review] diff allowlist"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I2 Step1 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I2 Step1: $f"
    exit 1
  fi
done

echo "[review] JSON parses"
python3 - <<'PY'
import json
json.load(open("schemas/closure_report_v1.schema.json","r",encoding="utf-8"))
print("schema json: ok")
PY

echo "[review] done"
