#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave18-mandate-types.md"
  "docs/contributing/SPLIT-CHECKLIST-mandate-types-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-mandate-types-step1.md"
  "scripts/ci/review-mandate-types-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, mandate ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave18 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave18 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-evidence/src/mandate/' >/dev/null; then
  echo "FAIL: Wave18 Step1 must not change crates/assay-evidence/src/mandate/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-evidence/src/mandate/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-evidence/src/mandate/** are not allowed in Wave18 Step1"
  git ls-files --others --exclude-standard -- 'crates/assay-evidence/src/mandate/**' | sed 's/^/  - /'
  exit 1
fi

cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings

cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_kind_serialization -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_mandate_builder -- --exact
cargo test -p assay-evidence --lib mandate::types::tests::test_operation_class_serialization -- --exact

echo "[review] PASS"
