#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ROADMAP.md"
  "scripts/ci/review-roadmap-route-governance-priorities.sh"
)

while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  [[ "$f" == .github/workflows/* ]] && {
    echo "FAIL: roadmap positioning slice must not touch workflows ($f)"
    exit 1
  }
  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  [[ "$ok" != "true" ]] && {
    echo "FAIL: file not allowed in roadmap positioning slice: $f"
    exit 1
  }
done < <(git diff --name-only "$BASE_REF"...HEAD)

rg -n 'tool taxonomy as first-class classes|Tool taxonomy as first-class classes' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing tool taxonomy priority"
  exit 1
}
rg -n 'Session identity \+ state store contract' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing session/state priority"
  exit 1
}
rg -n 'Coverage/completeness reports' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing coverage/completeness priority"
  exit 1
}
rg -n 'Not universal semantic-hijacking detection' docs/ROADMAP.md >/dev/null || {
  echo "FAIL: roadmap missing bounded non-play"
  exit 1
}

echo "[review] done"
