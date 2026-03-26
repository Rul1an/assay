#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step3.md"
  "scripts/ci/review-wave44-evaluate-kernel-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp/tool_call_handler"
  "crates/assay-core/tests"
  "crates/assay-cli/src/cli/commands"
  "crates/assay-mcp-server"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave44 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave44 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave44 Step3 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step3.md"

for marker in \
  'Wave44 Step2 shipped on `main` via `#958`.' \
  'keep `evaluate.rs` as the stable facade entrypoint' \
  'Step3 constraints:' \
  'no new module cuts' \
  'no behavior cleanup beyond internal follow-up notes'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/src/mcp/tool_call_handler/evaluate.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/evaluate_next/classification.rs' \
  'internal visibility tightening only if it requires no code edits in this wave' \
  'replay classification changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] pinned evaluate invariants"
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact

echo "[review] PASS"
