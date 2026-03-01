#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.lock"
  "crates/assay-adapter-ucp/Cargo.toml"
  "crates/assay-adapter-ucp/src/lib.rs"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_happy_discovery_requested.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_happy_order_requested.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_happy_checkout_updated.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_happy_fulfillment_updated.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_negative_missing_order_id.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_negative_invalid_event_type.json"
  "scripts/ci/fixtures/adr026/ucp/v2026-01-23/ucp_negative_malformed.json"
  "scripts/ci/test-adapter-ucp.sh"
  "scripts/ci/review-adr026-ucp-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 UCP Step2 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 UCP Step2: $f"
    exit 1
  fi
done

echo "[review] UCP contract markers"
rg -n '^name = "assay-adapter-ucp"$' crates/assay-adapter-ucp/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-ucp crate"
  exit 1
}
rg -n 'pub struct UcpAdapter' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing UcpAdapter type"
  exit 1
}
rg -n 'ConvertMode::Lenient' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing lenient-mode handling"
  exit 1
}
rg -n 'validate_json_shape' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing shared JSON shape validation"
  exit 1
}
rg -n 'digest_canonical_json' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing canonical digest tests"
  exit 1
}
rg -n 'assay.adapter.ucp.order.requested' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing order.requested mapping"
  exit 1
}
rg -n 'assay.adapter.ucp.message' crates/assay-adapter-ucp/src/lib.rs >/dev/null || {
  echo "FAIL: missing generic message mapping"
  exit 1
}

bash scripts/ci/test-adapter-ucp.sh >/dev/null

echo "[review] done"
