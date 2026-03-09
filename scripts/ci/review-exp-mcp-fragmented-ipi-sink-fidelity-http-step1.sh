#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md"
  "docs/contributing/SPLIT-CHECKLIST-sink-fidelity-http-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-sink-fidelity-http-step1.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-sink-fidelity-http-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, sink-failure subtree ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave22 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave22 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^scripts/ci/exp-mcp-fragmented-ipi/' >/dev/null; then
  echo "FAIL: Wave22 Step1 must not change scripts/ci/exp-mcp-fragmented-ipi/**"
  exit 1
fi

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh$' >/dev/null; then
  echo "FAIL: Wave22 Step1 must not change scripts/ci/test-exp-mcp-fragmented-ipi-sink-failure.sh"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under scripts/ci/exp-mcp-fragmented-ipi/** are not allowed in Wave22 Step1"
  git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] marker checks (run shape, fidelity constraints, metrics)"

rg -n 'RUNS_ATTACK.*2' docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md >/dev/null || {
  echo "FAIL: frozen RUNS_ATTACK=2 marker missing"
  exit 1
}
rg -n 'RUNS_LEGIT.*100' docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md >/dev/null || {
  echo "FAIL: frozen RUNS_LEGIT=100 marker missing"
  exit 1
}
rg -n 'success_any_sink_canary' docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md >/dev/null || {
  echo "FAIL: primary metric marker missing"
  exit 1
}
rg -n 'localhost-only|offline-only|deterministic|no external network dependency' \
  docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md >/dev/null || {
  echo "FAIL: frozen fidelity constraints markers missing"
  exit 1
}
rg -n 'egress_http_status_class|payload_delivered|response_observed' \
  docs/contributing/SPLIT-PLAN-wave22-sink-fidelity-http.md >/dev/null || {
  echo "FAIL: completion-layer publication markers missing"
  exit 1
}

cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact

echo "[review] PASS"
