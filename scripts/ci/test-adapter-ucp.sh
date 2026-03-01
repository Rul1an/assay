#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="scripts/ci/fixtures/adr026/ucp/v2026-01-23"

for f in \
  "$FIXTURE_DIR/ucp_happy_discovery_requested.json" \
  "$FIXTURE_DIR/ucp_happy_order_requested.json" \
  "$FIXTURE_DIR/ucp_happy_checkout_updated.json" \
  "$FIXTURE_DIR/ucp_happy_fulfillment_updated.json" \
  "$FIXTURE_DIR/ucp_negative_missing_order_id.json" \
  "$FIXTURE_DIR/ucp_negative_invalid_event_type.json" \
  "$FIXTURE_DIR/ucp_negative_malformed.json"
do
  test -f "$f"
done

cargo test -p assay-adapter-ucp
