#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-a2a-step2-mvp}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr026-a2a-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr026-a2a-step3.md"
  "scripts/ci/review-adr026-a2a-step3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 A2A Step3 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 A2A Step3: $f"
    exit 1
  fi
done

echo "[review] invariants"
test -f docs/architecture/PLAN-ADR-026-A2A-2026q2.md || { echo "FAIL: missing A2A plan"; exit 1; }
test -f crates/assay-adapter-api/src/lib.rs || { echo "FAIL: missing assay-adapter-api"; exit 1; }
test -f crates/assay-adapter-a2a/src/lib.rs || { echo "FAIL: missing assay-adapter-a2a"; exit 1; }
test -f scripts/ci/test-adapter-a2a.sh || { echo "FAIL: missing A2A test runner"; exit 1; }
test -f scripts/ci/fixtures/adr026/a2a/v0.2/a2a_happy_task_requested.json || { echo "FAIL: missing A2A happy fixture"; exit 1; }
test -f scripts/ci/fixtures/adr026/a2a/v0.2/a2a_negative_missing_task_id.json || { echo "FAIL: missing A2A negative fixture"; exit 1; }
rg -n 'pub trait ProtocolAdapter' crates/assay-adapter-api/src/lib.rs >/dev/null || { echo "FAIL: adapter trait missing"; exit 1; }
rg -n 'pub struct A2aAdapter' crates/assay-adapter-a2a/src/lib.rs >/dev/null || { echo "FAIL: A2aAdapter missing"; exit 1; }

bash scripts/ci/test-adapter-a2a.sh >/dev/null

echo "[review] done"
