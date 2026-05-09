#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

echo "[review] Week8 SOTA gate scripts syntax"
bash -n scripts/ci/optional-public-api-drift.sh
bash -n scripts/ci/mutation-smoke-pure-modules.sh

echo "[review] Week8 docs anchors"
rg 'cargo-semver-checks' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md >/dev/null
rg 'cargo-public-api' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md >/dev/null
rg 'cargo-mutants' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md >/dev/null
rg 'OWASP MCP Top 10' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md docs/security/OWASP-MCP-TOP10-TEST-MAP.md >/dev/null

for risk in MCP01 MCP02 MCP03 MCP05 MCP06 MCP08 MCP10; do
  rg "$risk" docs/security/OWASP-MCP-TOP10-TEST-MAP.md >/dev/null
 done

rg 'trust_basis/diff.rs' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md scripts/ci/mutation-smoke-pure-modules.sh >/dev/null
rg 'trust_basis/classifiers.rs' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md scripts/ci/mutation-smoke-pure-modules.sh >/dev/null
rg 'sandbox/degradation.rs' docs/contributing/SPLIT-PLAN-week8-sota-gates-2026q2.md scripts/ci/mutation-smoke-pure-modules.sh >/dev/null

echo "[review] optional scripts dry-run behavior without tool assumptions"
bash scripts/ci/optional-public-api-drift.sh
bash scripts/ci/mutation-smoke-pure-modules.sh

echo "[review] PASS"
