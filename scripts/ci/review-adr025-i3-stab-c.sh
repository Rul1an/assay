#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-CHECKLIST-adr025-i3-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-i3-step3.md"
  "docs/ops/ADR-025-I3-OTEL-BRIDGE-RUNBOOK.md"
  "scripts/ci/review-adr025-i3-stab-c.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: I3 Stab C must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    if [[ "$f" == "$a" ]]; then
      ok="true"
      break
    fi
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I3 Stab C: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] invariants"
# Existing assets from I3 Step2/Step3/StabB must exist.
test -f scripts/ci/adr025-otel-bridge.sh || { echo "FAIL: missing scripts/ci/adr025-otel-bridge.sh"; exit 1; }
test -f scripts/ci/test-adr025-otel-bridge.sh || { echo "FAIL: missing scripts/ci/test-adr025-otel-bridge.sh"; exit 1; }
test -f .github/workflows/adr025-nightly-otel-bridge.yml || { echo "FAIL: missing nightly otel workflow"; exit 1; }

# Ensure StabB edge-case fixtures are still referenced by tests.
rg -n "otel_input_multi_trace_unsorted\.json" scripts/ci/test-adr025-otel-bridge.sh >/dev/null || { echo "FAIL: missing multi-trace test reference"; exit 1; }
rg -n "otel_input_multi_span_unsorted\.json" scripts/ci/test-adr025-otel-bridge.sh >/dev/null || { echo "FAIL: missing multi-span test reference"; exit 1; }
rg -n "otel_input_events_unsorted\.json" scripts/ci/test-adr025-otel-bridge.sh >/dev/null || { echo "FAIL: missing events-order test reference"; exit 1; }
rg -n "otel_input_links_unsorted\.json" scripts/ci/test-adr025-otel-bridge.sh >/dev/null || { echo "FAIL: missing links-order test reference"; exit 1; }

# Ensure docs include stabilization coverage callouts.
rg -n "Stabilization coverage \(I3 Stab B\)" docs/contributing/SPLIT-CHECKLIST-adr025-i3-step3.md >/dev/null || { echo "FAIL: checklist missing Stab B coverage section"; exit 1; }
rg -n "I3 Stab B" docs/contributing/SPLIT-REVIEW-PACK-adr025-i3-step3.md >/dev/null || { echo "FAIL: review pack missing Stab B status"; exit 1; }

echo "[review] done"
