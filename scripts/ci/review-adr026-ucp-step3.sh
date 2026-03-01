#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-ucp-step2-mvp-v2}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr026-ucp-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr026-ucp-step3.md"
  "scripts/ci/review-adr026-ucp-step3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 UCP Step3 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 UCP Step3: $f"
    exit 1
  fi
done

echo "[review] invariants"
test -f docs/architecture/PLAN-ADR-026-UCP-2026q2.md || { echo "FAIL: missing UCP plan"; exit 1; }
test -f crates/assay-adapter-api/src/lib.rs || { echo "FAIL: missing assay-adapter-api"; exit 1; }
test -f crates/assay-adapter-ucp/src/lib.rs || { echo "FAIL: missing assay-adapter-ucp"; exit 1; }
test -f scripts/ci/test-adapter-ucp.sh || { echo "FAIL: missing UCP test runner"; exit 1; }
test -f scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_happy_order_requested.json || { echo "FAIL: missing UCP happy fixture"; exit 1; }
test -f scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_negative_missing_order_id.json || { echo "FAIL: missing UCP negative fixture"; exit 1; }
rg -n 'pub trait ProtocolAdapter' crates/assay-adapter-api/src/lib.rs >/dev/null || { echo "FAIL: adapter trait missing"; exit 1; }
rg -n 'pub struct UcpAdapter' crates/assay-adapter-ucp/src/lib.rs >/dev/null || { echo "FAIL: UcpAdapter missing"; exit 1; }
rg -n 'assay.adapter.ucp.order.requested' crates/assay-adapter-ucp/src/lib.rs >/dev/null || { echo "FAIL: UCP order mapping missing"; exit 1; }
rg -n 'max_json_depth|max_array_length|max_payload_bytes' crates/assay-adapter-ucp/src/lib.rs >/dev/null || { echo "FAIL: parser caps markers missing"; exit 1; }

bash scripts/ci/test-adapter-ucp.sh >/dev/null

echo "[review] done"
