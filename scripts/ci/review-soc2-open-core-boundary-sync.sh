#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ROADMAP.md"
  "docs/open-core.md"
  "scripts/ci/review-soc2-open-core-boundary-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: SOC2 open-core boundary sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in SOC2 open-core boundary sync: $f"
    exit 1
  fi
done

echo "[review] boundary markers"
rg -n '^\| \*\*Baseline Packs\*\* \| `eu-ai-act-baseline`, `soc2-baseline` \| Apache-2\.0 \|$' docs/open-core.md >/dev/null || {
  echo "FAIL: open-core baseline pack row must include soc2-baseline without coming-soon marker"
  exit 1
}
rg -n '^- \[x\] \*\*SOC2 Baseline \(OSS\)\*\*: Control mapping pack for Common Criteria \(implemented in PR #287\)$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP must mark SOC2 Baseline OSS as complete"
  exit 1
}
rg -n '^- \[ \] \*\*SOC2 Pro \(Enterprise\)\*\*: Assurance-depth pack content and workflows$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP must keep SOC2 Pro as enterprise pending"
  exit 1
}

echo "[review] done"
