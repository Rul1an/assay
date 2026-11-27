#!/usr/bin/env bash
# Real-input checks at commit and push; the mutation battery runs in CI, and at pre-push when its inputs change.
set -euo pipefail
[[ "$#" -eq 0 ]] || exit 2
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
python3 scripts/ci/check-assay-action-hook-stages.py
bash scripts/ci/check-assay-action-pin.sh
python3 scripts/ci/check-assay-action-consumer-compat.py
bash scripts/ci/test-action-discovery-junction.sh --live-only
