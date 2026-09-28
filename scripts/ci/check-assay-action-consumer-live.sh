#!/usr/bin/env bash
# Commit-time real-input checks; mutation batteries run at pre-push and in CI.
set -euo pipefail
[[ "$#" -eq 0 ]] || exit 2
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
python3 scripts/ci/check-assay-action-hook-stages.py
bash scripts/ci/check-assay-action-pin.sh
python3 scripts/ci/check-assay-action-consumer-compat.py
bash scripts/ci/test-action-discovery-junction.sh --live-only
