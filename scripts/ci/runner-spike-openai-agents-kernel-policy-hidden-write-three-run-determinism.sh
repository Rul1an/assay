#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"

ASSAY_RUNNER_OPENAI_AGENTS_SCENARIO=hidden_write \
  ASSAY_RUNNER_ACCEPTANCE_RUN_ID="${ASSAY_RUNNER_ACCEPTANCE_RUN_ID:-run_openai_agents_hidden_write_determinism}" \
  "$ROOT/scripts/ci/runner-spike-openai-agents-kernel-policy-three-run-determinism.sh"
