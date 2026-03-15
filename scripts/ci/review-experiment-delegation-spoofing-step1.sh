#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md"
  "docs/contributing/SPLIT-PLAN-experiment-delegation-spoofing.md"
  "docs/contributing/SPLIT-CHECKLIST-experiment-delegation-spoofing-step1.md"
  "scripts/ci/review-experiment-delegation-spoofing-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban + crate-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Step1 must not touch workflows ($f)"
    exit 1
  fi

  if [[ "$f" == crates/* ]]; then
    echo "FAIL: Step1 must not touch crates ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in delegation-spoofing Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

PLAN="docs/architecture/PLAN-EXPERIMENT-DELEGATION-SPOOFING-PROVENANCE-2026q2.md"

echo "[review] plan markers"
LITERAL_MARKERS=(
  'Capability Overclaim'
  'Provenance Ambiguity'
  'Delegation Identity Spoofing'
  'Preference/Selection Manipulation'
  'AdapterCapabilities'
  'RawPayloadRef'
  'ProtocolDescriptor'
  'LossinessLevel'
  'Condition A'
  'Condition B'
  'Condition C'
  'COR'
  'PBR'
  'ISSR'
  'SMR'
  'FPBR'
  'Control D1'
  'Control D2'
  'Control D3'
  'no_effect'
  'activation_with_correct_detection'
  'activation_with_trust_upgrade'
  'activation_with_selection_manipulation'
  'vector_id'
  'hypothesis_tags'
)
for marker in "${LITERAL_MARKERS[@]}"; do
  rg -Fn "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing literal marker in plan: $marker"
    exit 1
  }
done

REGEX_MARKERS=(
  'H1.*COR.*10%'
  'H2.*PBR.*5%'
  'H3.*FPBR.*2%'
  'H4.*V3.*highest ISSR'
)
for pattern in "${REGEX_MARKERS[@]}"; do
  rg -n "$pattern" "$PLAN" >/dev/null || {
    echo "FAIL: missing regex marker in plan: $pattern"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check

echo "[review] pinned adapter + evidence tests"
cargo test -p assay-adapter-api
cargo test -p assay-evidence

echo "[review] PASS"
