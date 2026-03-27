#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md"
  "docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step1.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step1.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step1.md"
  "scripts/ci/review-tr2-tool-call-handler-tests-step1.sh"
)

if ! git rev-parse --verify "${BASE_REF}^{commit}" >/dev/null 2>&1; then
  echo "BASE_REF does not resolve to a commit: $BASE_REF" >&2
  exit 1
fi

tmp_changed="$(mktemp)"
trap 'rm -f "$tmp_changed"' EXIT

git diff --name-only "$BASE_REF"...HEAD >"$tmp_changed"
git diff --name-only >>"$tmp_changed"
git diff --name-only --cached >>"$tmp_changed"
git ls-files --others --exclude-standard >>"$tmp_changed"

sort -u -o "$tmp_changed" "$tmp_changed"

while IFS= read -r file; do
  [[ -z "$file" ]] && continue

  if [[ "$file" == .github/workflows/* ]]; then
    echo "workflow file changed out of scope: $file" >&2
    exit 1
  fi

  allowed=false
  for allowed_file in "${ALLOWED_FILES[@]}"; do
    if [[ "$file" == "$allowed_file" ]]; then
      allowed=true
      break
    fi
  done

  if [[ "$allowed" == false ]]; then
    echo "out-of-scope file changed: $file" >&2
    exit 1
  fi
done <"$tmp_changed"

while IFS= read -r file; do
  [[ -z "$file" ]] && continue
  if [[ "$file" == crates/assay-core/src/mcp/tool_call_handler/* ]]; then
    echo "tool_call_handler sources must remain untouched in T-R2 Step1" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/tests/* ]]; then
    echo "assay-core integration tests must remain untouched in T-R2 Step1" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/policy/* ]]; then
    echo "policy sources must remain untouched in T-R2 Step1" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/decision.rs ]]; then
    echo "decision.rs must remain untouched in T-R2 Step1" >&2
    exit 1
  fi
done <"$tmp_changed"

cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_handler_emits_decision_on_policy_allow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegated_context_emits_typed_fields_for_supported_flow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_tool_drift_deny_emits_alert_obligation_outcome' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::test_lifecycle_emitter_not_called_when_none' -- --exact

echo "[review] PASS"
