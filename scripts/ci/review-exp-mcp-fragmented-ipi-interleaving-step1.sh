#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave23-interleaving.md"
  "docs/contributing/SPLIT-CHECKLIST-interleaving-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-interleaving-step1.md"
  "scripts/ci/review-exp-mcp-fragmented-ipi-interleaving-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave23 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave23 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] no-touch gates for experiment harness paths"
if git diff --name-only "$BASE_REF"...HEAD | rg -n '^scripts/ci/exp-mcp-fragmented-ipi/' >/dev/null; then
  echo "FAIL: Wave23 Step1 must not change scripts/ci/exp-mcp-fragmented-ipi/**"
  exit 1
fi

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^scripts/ci/test-exp-mcp-fragmented-ipi.*\.sh$' >/dev/null; then
  echo "FAIL: Wave23 Step1 must not change scripts/ci/test-exp-mcp-fragmented-ipi*.sh"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under scripts/ci/exp-mcp-fragmented-ipi/** are not allowed in Wave23 Step1"
  git ls-files --others --exclude-standard -- 'scripts/ci/exp-mcp-fragmented-ipi/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] hygiene checks"
cargo fmt --check
cargo clippy -p assay-cli -- -D warnings
cargo test -p assay-cli mcp_wrap_coverage_cli_smoke_writes_report -- --exact

echo "[review] PASS"
