#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/ops/ADR-025-I3-OTEL-RELEASE-RUNBOOK.md"
  "docs/contributing/SPLIT-CHECKLIST-adr025-i3-step4-c-closure.md"
  "docs/contributing/SPLIT-REVIEW-PACK-adr025-i3-step4-c-closure.md"
  "docs/architecture/PLAN-ADR-025-I3-otel-bridge-2026q2.md"
  "docs/ROADMAP.md"
  "scripts/ci/review-adr025-i3-step4-c.sh"
)

echo "[review] diff allowlist"
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
    echo "FAIL: Step4C must not change workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in I3 Step4C: $f"
    exit 1
  fi
done <<< "$changed"

echo "[review] invariants on main assets"
test -f schemas/otel_release_policy_v1.json || { echo "FAIL: missing otel_release_policy_v1.json"; exit 1; }
test -f scripts/ci/adr025-otel-release.sh || { echo "FAIL: missing adr025-otel-release.sh"; exit 1; }
test -f scripts/ci/review-adr025-i3-step4-b.sh || { echo "FAIL: missing review-adr025-i3-step4-b.sh"; exit 1; }
test -f .github/workflows/release.yml || { echo "FAIL: missing release.yml"; exit 1; }
test -f .github/workflows/adr025-nightly-otel-bridge.yml || { echo "FAIL: missing adr025-nightly-otel-bridge.yml"; exit 1; }

rg -n "adr025-otel-release\.sh" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must reference adr025-otel-release.sh"
  exit 1
}

rg -n "schemas/otel_release_policy_v1\.json" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must reference otel_release_policy_v1.json"
  exit 1
}

rg -n "ASSAY_OTEL_GATE" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must keep ASSAY_OTEL_GATE mode wiring"
  exit 1
}

rg -n "MODE:.*attach" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must keep default attach mode expression"
  exit 1
}

rg -n "name:\s*adr025-otel-bridge-release-evidence" .github/workflows/release.yml >/dev/null || {
  echo "FAIL: release.yml must upload adr025-otel-bridge-release-evidence"
  exit 1
}

rg -n "name:\s*adr025-otel-bridge-report" .github/workflows/adr025-nightly-otel-bridge.yml >/dev/null || {
  echo "FAIL: nightly workflow must keep artifact name adr025-otel-bridge-report"
  exit 1
}

rg -n "retention-days:\s*14" .github/workflows/adr025-nightly-otel-bridge.yml >/dev/null || {
  echo "FAIL: nightly workflow must keep retention-days: 14"
  exit 1
}

echo "[review] done"
