#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md"
  "docs/contributing/SPLIT-CHECKLIST-adr015-byos-phase1-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr015-byos-phase1-step1.md"
  "scripts/ci/review-adr015-phase1-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/config.rs"
  "crates/assay-core/src/model/types.rs"
  "crates/assay-evidence/src"
  "crates/assay-cli/src"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-015 Phase1 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-015 Phase1 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: ADR-015 Phase1 Step1 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for p in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
rg -n '^# SPLIT PLAN - ADR-015 BYOS Phase 1 Closure$' \
  docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

PLAN="docs/contributing/SPLIT-PLAN-adr015-byos-phase1-closure.md"

for marker in \
  'store-status' \
  'StoreConfig' \
  'StoreSpec' \
  '.assay/store.yaml' \
  'assay-store.yaml' \
  'ASSAY_STORE_URL' \
  'object_lock' \
  'AWS S3' \
  'Backblaze B2' \
  'MinIO' \
  'az://' \
  'gcs://'
do
  rg -Fn "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing literal marker in plan: $marker"
    exit 1
  }
done

REGEX_MARKERS=(
  'No change to.*EvalConfig'
)
for pattern in "${REGEX_MARKERS[@]}"; do
  rg -n "$pattern" "$PLAN" >/dev/null || {
    echo "FAIL: missing regex marker in plan: $pattern"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings

echo "[review] pinned evidence tests"
cargo test -p assay-evidence

echo "[review] PASS"
