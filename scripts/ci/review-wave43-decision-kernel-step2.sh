#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "crates/assay-core/src/mcp/decision.rs"
  "crates/assay-core/src/mcp/decision_next/mod.rs"
  "crates/assay-core/src/mcp/decision_next/event_types.rs"
  "crates/assay-core/src/mcp/decision_next/normalization.rs"
  "crates/assay-core/src/mcp/decision_next/builder.rs"
  "crates/assay-core/src/mcp/decision_next/emitters.rs"
  "crates/assay-core/src/mcp/decision_next/guard.rs"
  "docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step2.md"
  "scripts/ci/review-wave43-decision-kernel-step2.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/tests"
  "crates/assay-core/src/mcp/tool_call_handler"
  "crates/assay-core/src/mcp/policy"
  "crates/assay-cli/src/cli/commands"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave43 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave43 Step2: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave43 Step2 must not change frozen path: $p"
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
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step2.md"

for marker in \
  'decision_next/' \
  'event_types.rs' \
  'builder.rs' \
  'emitters.rs' \
  'guard.rs' \
  'normalization.rs' \
  'keep inline unit tests in `decision.rs`'
do
  rg -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/src/mcp/decision.rs' \
  'crates/assay-core/src/mcp/decision_next/event_types.rs' \
  'DecisionEmitterGuard::new' \
  'refresh_contract_projections' \
  'crates/assay-core/tests/decision_emit_invariant.rs'
do
  rg -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --all --check
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] pinned decision and replay tests"
cargo test -p assay-core --lib mcp::decision::tests::test_event_serialization -- --exact
cargo test -p assay-core --lib mcp::decision::tests::test_reason_codes_are_string_constants -- --exact
cargo test -p assay-core --lib mcp::decision::tests::test_decision_event_omits_delegation_fields_when_absent -- --exact
cargo test -p assay-core --test decision_emit_invariant test_policy_allow_emits_once -- --exact
cargo test -p assay-core --test decision_emit_invariant test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -p assay-core --test decision_emit_invariant test_guard_drop_emits_on_early_return -- --exact
cargo test -p assay-core --test decision_emit_invariant test_event_contains_required_fields -- --exact
cargo test -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact

echo "[review] PASS"
