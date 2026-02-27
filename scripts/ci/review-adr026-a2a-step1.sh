#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "Cargo.toml"
  "Cargo.lock"
  "crates/assay-adapter-a2a/Cargo.toml"
  "crates/assay-adapter-a2a/src/lib.rs"
  "docs/architecture/PLAN-ADR-026-A2A-2026q2.md"
  "scripts/ci/review-adr026-a2a-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-026 A2A Step1 must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-026 A2A Step1: $f"
    exit 1
  fi
done

echo "[review] contract markers"
rg -n '^name = "assay-adapter-a2a"$' crates/assay-adapter-a2a/Cargo.toml >/dev/null || {
  echo "FAIL: missing assay-adapter-a2a crate"
  exit 1
}
rg -n 'pub struct A2aAdapter' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: missing A2aAdapter type"
  exit 1
}
rg -n 'runtime translation is not implemented yet' crates/assay-adapter-a2a/src/lib.rs >/dev/null || {
  echo "FAIL: stubbed Step1 convert contract missing"
  exit 1
}
rg -n '^# PLAN — ADR-026 A2A Adapter Follow-up \(2026q2\)$' docs/architecture/PLAN-ADR-026-A2A-2026q2.md >/dev/null || {
  echo "FAIL: missing A2A plan title"
  exit 1
}
rg -n '^## Initial event families \(frozen for Step2\)$' docs/architecture/PLAN-ADR-026-A2A-2026q2.md >/dev/null || {
  echo "FAIL: missing initial event families section"
  exit 1
}

cargo test -p assay-adapter-a2a >/dev/null

echo "[review] done"
