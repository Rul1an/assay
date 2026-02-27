#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-016-Pack-Taxonomy.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr016-soc2-baseline-sync.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-016 SOC2 baseline sync must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-016 SOC2 baseline sync: $f"
    exit 1
  fi
done

echo "[review] status markers"
rg -n '^Accepted \(January 2026; boundary sync February 2026\)$' docs/architecture/ADR-016-Pack-Taxonomy.md >/dev/null || {
  echo "FAIL: ADR-016 status line not synced"
  exit 1
}
rg -n '^\| `soc2-baseline` \| SOC 2 Common Criteria baseline mapping \| Implemented \(see ADR-022\) \|$' docs/architecture/ADR-016-Pack-Taxonomy.md >/dev/null || {
  echo "FAIL: ADR-016 must mark soc2-baseline implemented"
  exit 1
}
rg -n '^- \[x\] \*\*SOC2 Baseline \(OSS\)\*\*: Common Criteria mapping delivered' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP must mark SOC2 Baseline OSS delivered"
  exit 1
}
rg -n '^\| \*\*Baseline Packs\*\* \| `eu-ai-act-baseline` \(Article 12 mapping, v2\.10\.0\), `soc2-baseline` \(Common Criteria baseline, ADR-022\) \|$' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: ROADMAP open-core table must include soc2-baseline"
  exit 1
}

echo "[review] done"
