#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave43-decision-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave43-decision-kernel-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave43-decision-kernel-step3.md"
  "scripts/ci/review-wave43-decision-kernel-step3.sh"
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
    echo "FAIL: Wave43 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave43 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave43 Step3 must not change frozen path: $p"
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
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave43-decision-kernel-step3.md"

for marker in \
  'Wave43 Step2 shipped on `main` via `#955`.' \
  'keep inline unit tests in `decision.rs`' \
  'Stabilization + micro-cleanup only:' \
  'no public contract drift' \
  'no new modules'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/src/mcp/decision.rs' \
  'crates/assay-core/src/mcp/decision_next/event_types.rs' \
  'internal visibility tightening' \
  'payload-shape changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] pinned decision and replay tests"
cargo test -p assay-core --lib mcp::decision::tests::test_event_serialization -- --exact
cargo test -p assay-core --lib mcp::decision::tests::test_reason_codes_are_string_constants -- --exact
cargo test -p assay-core --test decision_emit_invariant test_policy_allow_emits_once -- --exact
cargo test -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact

echo "[review] PASS"
