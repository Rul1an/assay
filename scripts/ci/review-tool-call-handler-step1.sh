#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave16-tool-call-handler.md"
  "docs/contributing/SPLIT-CHECKLIST-tool-call-handler-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tool-call-handler-step1.md"
  "scripts/ci/review-tool-call-handler-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, mcp ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave16 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave16 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-core/src/mcp/' >/dev/null; then
  echo "FAIL: Wave16 Step1 must not change crates/assay-core/src/mcp/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/src/mcp/** are not allowed in Wave16 Step1"
  git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] gates"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact

echo "[review] PASS"
