#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ROADMAP.md"
  "scripts/ci/review-adr025-roadmap-open-core-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: roadmap open-core sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-025 roadmap open-core sync: $f"
    exit 1
  fi
done

echo "[review] roadmap ADR-025 H completion markers"
rg -n '^### H\. Audit Kit & Closure \(P2\) \[ADR-025\] ✅ Complete$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: missing complete marker on ADR-025 H heading"
  exit 1
}
rg -n '^- \[x\] \*\*Manifest Extensions\*\*: `x-assay\.packs_applied` and `mappings` for provenance \(I2\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: Manifest Extensions must be marked complete"
  exit 1
}
rg -n '^- \[x\] \*\*Completeness\*\*: Pack-relative signal gaps \(`required` vs `captured`\) \(I2\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: Completeness must be marked complete"
  exit 1
}
rg -n '^- \[x\] \*\*Closure Score\*\*: Replay-relative score \(0\.0-1\.0\) for hermetic replay readiness \(I2\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: Closure Score must be marked complete"
  exit 1
}
rg -n '^- \[x\] \*\*OTEL Bridge\*\*: Export Assay events to OTLP/GenAI SemConv \(Iteration 3\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: OTEL Bridge must be marked complete"
  exit 1
}

echo "[review] done"
