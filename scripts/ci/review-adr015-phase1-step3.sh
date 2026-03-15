#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-015-BYOS-Storage-Strategy.md"
  "docs/architecture/GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr015-phase1-step3.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban + crate-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step3 must not touch workflows ($f)"
    exit 1
  fi

  if [[ "$f" == crates/* ]]; then
    echo "FAIL: Step3 must not touch crates ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-015 Phase1 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] ADR-015 Phase 1 closure markers"
rg -Fn "Phase 1: BYOS CLI (Q2 2026) ✅ Complete" \
  docs/architecture/ADR-015-BYOS-Storage-Strategy.md >/dev/null || {
  echo "FAIL: ADR-015 Phase 1 not marked complete"
  exit 1
}

rg -Fn "Phase 1 is complete" docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP BYOS not marked complete"
  exit 1
}

rg -Fn "ADR-015 Phase 1 product closure" \
  docs/architecture/GAP-ASSAY-ARCHITECTURE-ROADMAP-2026q2.md >/dev/null || {
  echo "FAIL: GAP doc not updated"
  exit 1
}

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-evidence --all-targets -- -D warnings

echo "[review] pinned evidence tests"
cargo test -p assay-evidence

echo "[review] PASS"
