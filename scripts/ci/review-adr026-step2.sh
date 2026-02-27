#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/codex/adr026-step1-freeze}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.toml"
  "Cargo.lock"
  "crates/assay-adapter-acp/Cargo.toml"
  "crates/assay-adapter-acp/src/lib.rs"
  "scripts/ci/fixtures/adr026/acp/v2.11.0/acp_happy_intent_created.json"
  "scripts/ci/fixtures/adr026/acp/v2.11.0/acp_happy_checkout_requested.json"
  "scripts/ci/fixtures/adr026/acp/v2.11.0/acp_negative_missing_packet_id.json"
  "scripts/ci/fixtures/adr026/acp/v2.11.0/acp_negative_invalid_event_type.json"
  "scripts/ci/fixtures/adr026/acp/v2.11.0/acp_negative_malformed.json"
  "scripts/ci/test-adapter-acp.sh"
  "scripts/ci/review-adr026-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 Step2 must not change workflows ($f)"
    exit 1
  fi
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 Step2: $f"
    exit 1
  fi
done

echo "[review] ACP contract markers"
rg -n '^name = "assay-adapter-acp"$' crates/assay-adapter-acp/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-acp crate"
  exit 1
}
rg -n 'pub struct AcpAdapter' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: missing AcpAdapter type"
  exit 1
}
rg -n 'ConvertMode::Lenient' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: missing lenient-mode handling"
  exit 1
}
rg -n 'StrictLossinessViolation' crates/assay-adapter-acp/src/lib.rs >/dev/null || {
  echo "FAIL: strict lossiness contract missing"
  exit 1
}

bash scripts/ci/test-adapter-acp.sh >/dev/null

echo "[review] done"
