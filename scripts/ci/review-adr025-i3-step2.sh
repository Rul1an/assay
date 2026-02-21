#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOW_PREFIXES=(
  "scripts/ci/adr025-otel-bridge.sh"
  "scripts/ci/test-adr025-otel-bridge.sh"
  "scripts/ci/fixtures/adr025-i3/"
  "scripts/ci/review-adr025-i3-step2.sh"
)

echo "[review] allowlist + workflow-ban"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I3 Step2 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for p in "${ALLOW_PREFIXES[@]}"; do
    if [[ "$p" == */ ]]; then
      [[ "$f" == "$p"* ]] && ok="true"
    else
      [[ "$f" == "$p" ]] && ok="true"
    fi
    [[ "$ok" == "true" ]] && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I3 Step2: $f"
    exit 1
  fi
done

echo "[review] run otel bridge tests"
bash scripts/ci/test-adr025-otel-bridge.sh

echo "[review] done"
