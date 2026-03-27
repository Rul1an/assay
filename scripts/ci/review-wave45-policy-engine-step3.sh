#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave45-policy-engine.md"
  "docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step3.md"
  "scripts/ci/review-wave45-policy-engine-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp/policy"
  "crates/assay-core/tests"
  "crates/assay-core/src/mcp/tool_call_handler"
  "crates/assay-cli/src/cli/commands"
  "crates/assay-evidence"
)

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave45 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave45 Step3: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: Wave45 Step3 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-wave45-policy-engine.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step3.md"

for marker in \
  'Wave45 Step2 shipped on `main` via `#961`.' \
  'keep `engine.rs` as the stable facade entrypoint' \
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
  'crates/assay-core/src/mcp/policy/engine.rs' \
  'crates/assay-core/src/mcp/policy/engine_next/precedence.rs' \
  'crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs' \
  'internal visibility tightening only if it requires no code edits in this wave' \
  'reason-code or precedence changes'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] repo checks"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

echo "[review] pinned policy invariants"
cargo test -q -p assay-core --test policy_engine_test test_mixed_tools_config -- --exact
cargo test -q -p assay-core --test policy_engine_test test_constraint_enforcement -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval::approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope::restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redaction::redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --lib 'mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only' -- --exact

echo "[review] PASS"
