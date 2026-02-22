#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-INDEX.md"
  "scripts/ci/review-adr025-p3-index-b.sh"
)

changed="$(git diff --name-only "$BASE_REF"...HEAD)"

allowlisted_untracked=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  for a in "${ALLOWLIST[@]}"; do
    if [[ "$f" == "$a" ]]; then
      if [[ -n "$allowlisted_untracked" ]]; then
        allowlisted_untracked+=$'\n'
      fi
      allowlisted_untracked+="$f"
      break
    fi
  done
done < <(git ls-files --others --exclude-standard)

if [[ -n "$allowlisted_untracked" ]]; then
  if [[ -n "$changed" ]]; then
    changed="$(printf "%s\n%s\n" "$changed" "$allowlisted_untracked")"
  else
    changed="$allowlisted_untracked"
  fi
fi

if [[ -z "$changed" ]]; then
  echo "FAIL: no changes detected vs $BASE_REF"
  exit 1
fi

git diff --name-only "$BASE_REF"...HEAD | while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-025 P3 PR-B must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done
  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-025 P3 PR-B: $f"
    exit 1
  fi
done

rg -n "I1|Iteration 1" docs/architecture/ADR-025-INDEX.md >/dev/null || {
  echo "FAIL: index missing I1 section"
  exit 1
}
rg -n "I2|Iteration 2" docs/architecture/ADR-025-INDEX.md >/dev/null || {
  echo "FAIL: index missing I2 section"
  exit 1
}
rg -n "I3|Iteration 3" docs/architecture/ADR-025-INDEX.md >/dev/null || {
  echo "FAIL: index missing I3 section"
  exit 1
}

echo "[review] done"
