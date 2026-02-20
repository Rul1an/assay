#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

echo "[review] allowlist + no workflow changes"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I2 Step2 must not change workflows ($f)"
    exit 1
  fi

  case "$f" in
    scripts/ci/adr025-i2-closure-evaluate.sh|\
    scripts/ci/test-adr025-i2-closure-evaluate.sh|\
    scripts/ci/review-adr025-i2-step2.sh|\
    scripts/ci/fixtures/adr025-i2/*)
      ;;
    *)
      echo "FAIL: file not allowed in I2 Step2: $f"
      exit 1
      ;;
  esac
done

echo "[review] ensure scripts are executable"
test -x scripts/ci/adr025-i2-closure-evaluate.sh
test -x scripts/ci/test-adr025-i2-closure-evaluate.sh

echo "[review] run closure evaluator tests"
bash scripts/ci/test-adr025-i2-closure-evaluate.sh

echo "[review] done"
