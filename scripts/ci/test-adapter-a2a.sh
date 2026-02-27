#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

FIXTURE_DIR="scripts/ci/fixtures/adr026/a2a/v0.2"

for f in \
  "$FIXTURE_DIR/a2a_happy_agent_capabilities.json" \
  "$FIXTURE_DIR/a2a_happy_task_requested.json" \
  "$FIXTURE_DIR/a2a_happy_artifact_shared.json" \
  "$FIXTURE_DIR/a2a_negative_missing_task_id.json" \
  "$FIXTURE_DIR/a2a_negative_invalid_event_type.json" \
  "$FIXTURE_DIR/a2a_negative_malformed.json"
do
  test -f "$f"
done

cargo test -p assay-adapter-a2a
