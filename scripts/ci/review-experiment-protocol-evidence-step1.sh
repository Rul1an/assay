#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md"
  "docs/contributing/SPLIT-PLAN-experiment-protocol-evidence-interpretation.md"
  "docs/contributing/SPLIT-CHECKLIST-experiment-protocol-evidence-interpretation-step1.md"
  "scripts/ci/review-experiment-protocol-evidence-step1.sh"
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
    echo "FAIL: file not allowed in protocol-evidence Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

PLAN="docs/architecture/PLAN-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md"

echo "[review] plan markers"
LITERAL_MARKERS=(
  'Partial-Field Trust Read'
  'Precedence Inversion'
  'Compat Flattening'
  'Projection Loss'
  'consumer_read_path'
  'consumer_payload_state'
  'decision_outcome_kind'
  'deny_classification_source'
  'restrictiveness_rank'
  'same_effective_decision_class'
  'required_consumer_fields'
  'Condition A'
  'Condition B'
  'Condition C'
  'CDR'
  'PIR'
  'CFR'
  'PLR'
  'CCAR'
  'FPBR'
  'Control E1'
  'Control E2'
  'Control E3'
  'no_effect'
  'silent_downgrade'
  'silent_trust_upgrade'
  'Verified'
  'Self-reported'
  'Inferred'
  'consumer_realistic'
  'producer_realistic'
  'adapter_realistic'
)
for marker in "${LITERAL_MARKERS[@]}"; do
  rg -Fn "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing literal marker in plan: $marker"
    exit 1
  }
done

REGEX_MARKERS=(
  'H1.*CDR.*PIR.*10%'
  'H2.*5%'
  'H3.*FPBR.*2%'
  'H4.*V3.*highest CFR'
)
for pattern in "${REGEX_MARKERS[@]}"; do
  rg -n "$pattern" "$PLAN" >/dev/null || {
    echo "FAIL: missing regex marker in plan: $pattern"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check

echo "[review] pinned consumer/replay/deny contract tests"
cargo test -p assay-core --test replay_diff_contract
cargo test -p assay-core --test decision_emit_invariant
cargo test -p assay-core --test fulfillment_normalization

echo "[review] PASS"
