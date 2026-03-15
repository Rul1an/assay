#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/architecture/PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md"
  "docs/contributing/SPLIT-PLAN-experiment-memory-poison.md"
  "docs/contributing/SPLIT-CHECKLIST-experiment-memory-poison-step1.md"
  "scripts/ci/review-experiment-memory-poison-step1.sh"
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
    echo "FAIL: file not allowed in experiment memory-poison Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

PLAN="docs/architecture/PLAN-EXPERIMENT-MEMORY-POISON-DELAYED-TRIGGER-2026q2.md"

echo "[review] plan markers"
LITERAL_MARKERS=(
  'Overarching invariant'
  'schema-valid'
  'internally consistent'
  'restrictiveness rank'
  'classify_replay_diff'
  'project_context_contract'
  'state_snapshot_id'
  'DECAY_RUNS'
  'Condition A'
  'Condition B'
  'Condition C'
  'PRR'
  'DASR'
  'PPI'
  'RDCS'
  'FPBR'
  'Control B1'
  'Control B2'
  'Control B3'
  'no_effect'
  'retained_no_activation'
  'activation_with_correct_detection'
  'activation_with_misclassification'
  'activation_with_policy_shift'
  'vector_id'
  'hypothesis_tags'
)
for marker in "${LITERAL_MARKERS[@]}"; do
  rg -Fn "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

REGEX_MARKERS=(
  'H1.*PRR.*10%'
  'H2.*DASR.*5%'
  'H3.*FPBR.*2%'
  'H4.*Vector 4.*highest PRR'
)
for pattern in "${REGEX_MARKERS[@]}"; do
  rg -n "$pattern" "$PLAN" >/dev/null || {
    echo "FAIL: missing regex marker in plan: $pattern"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check

echo "[review] pinned contract tests"
cargo test -p assay-core --test replay_diff_contract
cargo test -p assay-core --test decision_emit_invariant
cargo test -p assay-core --test fulfillment_normalization

echo "[review] PASS"
