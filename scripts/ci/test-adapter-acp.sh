#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="scripts/ci/fixtures/adr026/acp/v2.11.0"

for f in \
  "$FIXTURE_DIR/acp_happy_intent_created.json" \
  "$FIXTURE_DIR/acp_happy_checkout_requested.json" \
  "$FIXTURE_DIR/acp_negative_missing_packet_id.json" \
  "$FIXTURE_DIR/acp_negative_invalid_event_type.json" \
  "$FIXTURE_DIR/acp_negative_malformed.json"
  do
  test -f "$f"
done

cargo test -p assay-adapter-acp
