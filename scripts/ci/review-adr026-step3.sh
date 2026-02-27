#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-step2-acp-mvp}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr026-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr026-step3.md"
  "scripts/ci/review-adr026-step3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 Step3 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 Step3: $f"
    exit 1
  fi
done

echo "[review] invariants"
test -f crates/assay-adapter-api/src/lib.rs || { echo "FAIL: missing assay-adapter-api"; exit 1; }
test -f crates/assay-adapter-acp/src/lib.rs || { echo "FAIL: missing assay-adapter-acp"; exit 1; }
test -f scripts/ci/test-adapter-acp.sh || { echo "FAIL: missing ACP test runner"; exit 1; }
test -f scripts/ci/fixtures/adr026/acp/v2.11.0/acp_happy_intent_created.json || { echo "FAIL: missing happy fixture"; exit 1; }
test -f scripts/ci/fixtures/adr026/acp/v2.11.0/acp_negative_missing_packet_id.json || { echo "FAIL: missing negative fixture"; exit 1; }
rg -n 'pub trait ProtocolAdapter' crates/assay-adapter-api/src/lib.rs >/dev/null || { echo "FAIL: adapter trait missing"; exit 1; }
rg -n 'pub struct AcpAdapter' crates/assay-adapter-acp/src/lib.rs >/dev/null || { echo "FAIL: AcpAdapter missing"; exit 1; }

bash scripts/ci/test-adapter-acp.sh >/dev/null

echo "[review] done"
