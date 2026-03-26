#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

line_count() {
  wc -l < "$1" | tr -d '[:space:]'
}

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-wave44-evaluate-kernel.md"
  "docs/contributing/SPLIT-CHECKLIST-wave44-evaluate-kernel-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave44-evaluate-kernel-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave44-evaluate-kernel-step1.md"
  "scripts/ci/review-wave44-evaluate-kernel-step1.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, tool_call_handler ban, tests ban)"

while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave44 Step1 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave44 Step1: $f"
    exit 1
  fi
done < <(git diff --name-only "$BASE_REF"...HEAD)

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-core/src/mcp/tool_call_handler/' >/dev/null; then
  echo "FAIL: Wave44 Step1 must not change crates/assay-core/src/mcp/tool_call_handler/**"
  exit 1
fi

if git diff --name-only "$BASE_REF"...HEAD | rg -n '^crates/assay-core/tests/' >/dev/null; then
  echo "FAIL: Wave44 Step1 must not change crates/assay-core/tests/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/tool_call_handler/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/src/mcp/tool_call_handler/** are not allowed in Wave44 Step1"
  git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/tool_call_handler/**' | sed 's/^/  - /'
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/tests/** are not allowed in Wave44 Step1"
  git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] no-drift baseline"
[[ "$(line_count crates/assay-core/src/mcp/tool_call_handler/evaluate.rs)" == "1016" ]] || {
  echo "FAIL: evaluate.rs LOC drifted during Step1"
  exit 1
}
[[ "$(line_count crates/assay-core/src/mcp/tool_call_handler/tests.rs)" == "1242" ]] || {
  echo "FAIL: tool_call_handler/tests.rs LOC drifted during Step1"
  exit 1
}
[[ "$(line_count crates/assay-core/tests/decision_emit_invariant.rs)" == "1293" ]] || {
  echo "FAIL: decision_emit_invariant.rs LOC drifted during Step1"
  exit 1
}
[[ "$(line_count crates/assay-core/tests/fulfillment_normalization.rs)" == "165" ]] || {
  echo "FAIL: fulfillment_normalization.rs LOC drifted during Step1"
  exit 1
}
[[ "$(line_count crates/assay-core/tests/replay_diff_contract.rs)" == "382" ]] || {
  echo "FAIL: replay_diff_contract.rs LOC drifted during Step1"
  exit 1
}
[[ "$(line_count crates/assay-core/tests/tool_taxonomy_policy_match.rs)" == "133" ]] || {
  echo "FAIL: tool_taxonomy_policy_match.rs LOC drifted during Step1"
  exit 1
}

echo "[review] gates"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test fulfillment_normalization fulfillment_normalizes_outcomes_and_sets_policy_deny_path -- --exact
cargo test -q -p assay-core --test replay_diff_contract classify_replay_diff_unchanged -- --exact

echo "[review] PASS"
