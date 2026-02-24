#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/ADR-025-INDEX.md"
  "docs/contributing/SPLIT-CHECKLIST-adr025-p3-index-c.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-p3-index-c.md"
  "docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr025-p3-index-c.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF"
changed="$(git diff --name-only "$BASE_REF"...HEAD)"

allowlisted_untracked=""
while IFS= read -r f; do
  [[ -z "$f" ]] && continue
  if [[ "$f" == .github/workflows/* ]]; then
    if [[ -n "$allowlisted_untracked" ]]; then
      allowlisted_untracked+=$'\n'
    fi
    allowlisted_untracked+="$f"
    continue
  fi

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

while IFS= read -r f; do
  [[ -z "$f" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: ADR-025 P3 PR-C must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in ADR-025 P3 PR-C: $f"
    exit 1
  fi
done <<< "$changed"

echo "[review] invariants"
test -f docs/architecture/ADR-025-INDEX.md || { echo "FAIL: missing ADR-025 index"; exit 1; }
test -f scripts/ci/review-adr025-p3-index-a.sh || { echo "FAIL: missing P3 A reviewer gate"; exit 1; }
test -f scripts/ci/review-adr025-p3-index-b.sh || { echo "FAIL: missing P3 B reviewer gate"; exit 1; }

rg -n "### I1" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing I1 section"; exit 1; }
rg -n "### I2" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing I2 section"; exit 1; }
rg -n "### I3" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing I3 section"; exit 1; }

rg -n "adr025-nightly-soak\.yml" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing soak workflow link"; exit 1; }
rg -n "adr025-nightly-closure\.yml" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing closure workflow link"; exit 1; }
rg -n "adr025-nightly-otel-bridge\.yml" docs/architecture/ADR-025-INDEX.md >/dev/null || { echo "FAIL: index missing OTel workflow link"; exit 1; }

rg -n "ADR-025 P3 status" docs/ROADMAP.md >/dev/null || { echo "FAIL: ROADMAP missing ADR-025 P3 status sync"; exit 1; }
rg -n "Step4C: .*complete on main|Step4C: .*complete on \`main\`" docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md >/dev/null || {
  echo "FAIL: I3 plan status sync missing Step4C complete-on-main wording"
  exit 1
}

echo "[review] done"
