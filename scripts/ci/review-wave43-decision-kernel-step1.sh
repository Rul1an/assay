#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step1.md"
  "scripts/ci/review-wave43-decision-kernel-step1.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp"
  "crates/assay-core/tests"
  "crates/assay-cli/src/cli/commands"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave43 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave43 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave43 Step1 must not change frozen path: $p"
    git diff --name-only "$BASE_REF"...HEAD -- "$p"
    exit 1
  fi
done

echo "[review] frozen paths must not contain untracked files"
for p in "${FROZEN_PATHS[@]}"; do
  if git ls-files --others --exclude-standard -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: untracked files present under frozen path: $p"
    git ls-files --others --exclude-standard -- "$p" | sed 's/^/  - /'
    exit 1
  fi
done

echo "[review] marker checks"
PLAN="docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step1.md"

rg -n '^# SPLIT PLAN - Wave43 Decision Kernel Split$' "$PLAN" >/dev/null || {
  echo "FAIL: missing plan title"
  exit 1
}

for marker in \
  'DecisionEvent' \
  'DecisionData' \
  'DecisionEmitter' \
  'DecisionEmitterGuard' \
  'reason_codes' \
  'consumer_contract' \
  'context_contract' \
  'deny_convergence' \
  'outcome_convergence' \
  'replay_compat' \
  'replay_diff' \
  'decision_next/' \
  'event_types.rs' \
  'builder.rs' \
  'emitters.rs' \
  'guard.rs' \
  'normalization.rs' \
  'No event payload shape changes are allowed in this wave.' \
  'No reason-code renames are allowed in this wave.' \
  'No replay-basis behavior changes are allowed in this wave.'
do
  rg -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/src/mcp/decision.rs' \
  'crates/assay-core/src/mcp/decision_next/mod.rs' \
  'crates/assay-core/tests/decision_emit_invariant.rs' \
  'crates/assay-core/tests/fulfillment_normalization.rs'
do
  rg -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] pinned decision tests"
cargo test -p assay-core test_event_serialization -- --exact
cargo test -p assay-core test_reason_codes_are_string_constants -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_policy_allow_emits_once -- --exact
cargo test -p assay-core --test decision_emit_invariant delegation::test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -p assay-core --test decision_emit_invariant guard::test_guard_drop_emits_on_early_return -- --exact
cargo test -p assay-core --test decision_emit_invariant emission::test_event_contains_required_fields -- --exact
cargo test -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact

echo "[review] PASS"
