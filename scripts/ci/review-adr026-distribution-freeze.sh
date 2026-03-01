#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-026-Adapter-Distribution-Policy.md"
  "scripts/ci/review-adr026-distribution-freeze.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 distribution freeze must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 distribution freeze: $f"
    exit 1
  fi
done

echo "[review] policy markers"
rg -n '^# ADR-026 Adapter Distribution Policy \(v1\)$' \
  docs/architecture/ADR-026-Adapter-Distribution-Policy.md >/dev/null || {
  echo "FAIL: policy doc missing title"
  exit 1
}

rg -n 'no adapter crate is published to crates.io yet' \
  docs/architecture/ADR-026-Adapter-Distribution-Policy.md >/dev/null || {
  echo "FAIL: policy doc missing crates.io freeze statement"
  exit 1
}

echo "[review] release invariants"
rg -n 'Publish Crates \(Idempotent\)' .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release workflow no longer uses idempotent publish step"
  exit 1
}

if rg -n 'assay-adapter-(api|acp|a2a|ucp)' scripts/ci/publish_idempotent.sh >/dev/null; then
  echo "FAIL: adapter crates must not be in publish_idempotent.sh for this freeze"
  exit 1
fi

echo "[review] done"
