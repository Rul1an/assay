#!/usr/bin/env bash
# Owned producer for action-v2-test default-discovery-sandbox-junction (#2778).
# Single nested-sandbox recipe source; do not inline a second copy in the workflow.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

bash -c "$(cat "${ROOT}/scripts/ci/fixtures/assay-action-pin/remediation_recipe.cmd")"
test -f .assay/evidence/nested/sandbox.tar.gz
