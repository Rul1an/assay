#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave20-sink-failure-partial.md"
  "docs/contributing/SPLIT-CHECKLIST-sink-failure-partial-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-sink-failure-partial-step1.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-sink-failure-partial-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, sink-failure subtree ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave20 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave20 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^scripts/ci/exp-mcp-fragmented-ipi/' >/dev/null; then
  echo "FAIL: Wave20 Step1 must not change scripts/ci/exp-mcp-fragmented-ipi/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under scripts/ci/exp-mcp-fragmented-ipi/** are not allowed in Wave20 Step1"
  git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | sed 's/^/  - /'
  exit 1
fi

cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact

echo "[review] PASS"
