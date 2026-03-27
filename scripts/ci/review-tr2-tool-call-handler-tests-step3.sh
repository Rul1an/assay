#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

ALLOWLIST=(
  "docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md"
  "docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step3.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step3.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step3.md"
  "scripts/ci/review-tr2-tool-call-handler-tests-step3.sh"
)

FROZEN_PATHS=(
  "crates/assay-core/src/mcp/tool_call_handler"
  "crates/assay-core/tests"
  "crates/assay-core/src/mcp/policy"
  "crates/assay-core/src/mcp/decision.rs"
)

changed_files() {
  git diff --name-only "$BASE_REF"...HEAD || true
  git diff --name-only || true
  git diff --name-only --cached || true
  git ls-files --others --exclude-standard || true
}

echo "[review] allowlist-only diff vs $BASE_REF + workflow-ban"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: T-R2 Step3 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in T-R2 Step3: $f"
    exit 1
  fi
done < <(changed_files | awk 'NF' | sort -u)

echo "[review] frozen tracked paths must not change"
for p in "${FROZEN_PATHS[@]}"; do
  if git diff --name-only "$BASE_REF"...HEAD -- "$p" | rg -n '.' >/dev/null; then
    echo "FAIL: T-R2 Step3 must not change frozen path: $p"
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
PLAN="docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md"
MOVE_MAP="docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step3.md"

for marker in \
  'T-R2 Step1 shipped on `main` via `#983`.' \
  'T-R2 Step2 shipped on `main` via `#984`.' \
  'keep `tests/mod.rs` as the stable unit-test root' \
  'no promotion into `crates/assay-core/tests/**`' \
  'no new module cuts' \
  'no drift in private-access coverage shape'
do
  rg -F -n "$marker" "$PLAN" >/dev/null || {
    echo "FAIL: missing marker in plan: $marker"
    exit 1
  }
done

for marker in \
  'crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs' \
  'crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs' \
  'future helper cleanup or selector hygiene that reaches beyond closure-only docs/gates requires a separate wave' \
  'no private-access widening, production behavior cleanup, or test-family reinterpretation is part of Step3'
do
  rg -F -n "$marker" "$MOVE_MAP" >/dev/null || {
    echo "FAIL: missing marker in move-map: $marker"
    exit 1
  }
done

echo "[review] root shape checks"
for required in \
  '^mod approval;$' \
  '^mod classification;$' \
  '^mod delegation;$' \
  '^mod emission;$' \
  '^mod fixtures;$' \
  '^mod lifecycle;$' \
  '^mod redaction;$' \
  '^mod scope;$'
do
  if ! rg -n "$required" crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs >/dev/null; then
    echo "FAIL: tests/mod.rs is missing required module declaration: $required"
    exit 1
  fi
done

if rg -n '^#\[test\]|^fn test_|^fn approval_required_|^fn restrict_scope_|^fn redact_args_' \
  crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs >/dev/null; then
  echo "FAIL: tests/mod.rs must remain a thin module root"
  exit 1
fi

echo "[review] repo checks"
cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings

echo "[review] pinned T-R2 invariants"
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::emission::test_handler_emits_decision_on_policy_allow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegation::delegated_context_emits_typed_fields_for_supported_flow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::scope::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redaction::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::classification::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none' -- --exact

echo "[review] PASS"
