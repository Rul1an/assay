#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-core/src/mcp/tool_call_handler/tests.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs"
  "docs/contributing/SPLIT-PLAN-tr2-tool-call-handler-tests.md"
  "docs/contributing/SPLIT-CHECKLIST-tr2-tool-call-handler-tests-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-tr2-tool-call-handler-tests-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-tr2-tool-call-handler-tests-step2.md"
  "scripts/ci/review-tr2-tool-call-handler-tests-step2.sh"
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
  if [[ "$file" == crates/assay-core/tests/* ]]; then
    echo "assay-core integration tests must remain untouched in T-R2 Step2" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/policy/* ]]; then
    echo "policy sources must remain untouched in T-R2 Step2" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/decision.rs ]]; then
    echo "decision.rs must remain untouched in T-R2 Step2" >&2
    exit 1
  fi
  if [[ "$file" == crates/assay-core/src/mcp/tool_call_handler/* ]] && \
     [[ "$file" != crates/assay-core/src/mcp/tool_call_handler/tests.rs ]] && \
     [[ "$file" != crates/assay-core/src/mcp/tool_call_handler/tests/* ]]; then
    echo "non-test tool_call_handler source changed out of scope: $file" >&2
    exit 1
  fi
done <"$tmp_changed"

if [[ -e crates/assay-core/src/mcp/tool_call_handler/tests.rs ]]; then
  echo "tests.rs must be replaced by tests/mod.rs in T-R2 Step2" >&2
  exit 1
fi

for required in \
  '^mod fixtures;$' \
  '^mod emission;$' \
  '^mod delegation;$' \
  '^mod approval;$' \
  '^mod scope;$' \
  '^mod redaction;$' \
  '^mod classification;$' \
  '^mod lifecycle;$'
do
  if ! rg -n "$required" crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs >/dev/null; then
    echo "tests/mod.rs is missing required module declaration: $required" >&2
    exit 1
  fi
done

if rg -n '^#\[test\]|^fn test_|^fn approval_required_|^fn restrict_scope_|^fn redact_args_' \
  crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs >/dev/null; then
  echo "tests/mod.rs must remain a thin module root" >&2
  exit 1
fi

RUST_SCOPE_FILES=(
  "crates/assay-core/src/mcp/tool_call_handler/tests.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/fixtures.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/approval.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/classification.rs"
  "crates/assay-core/src/mcp/tool_call_handler/tests/lifecycle.rs"
)

count_base_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if git cat-file -e "$BASE_REF:$file" 2>/dev/null; then
      count=$(git show "$BASE_REF:$file" | rg -o "$pattern" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

count_head_matches() {
  local pattern="$1"
  local total=0
  local count
  for file in "${RUST_SCOPE_FILES[@]}"; do
    if [[ -f "$file" ]]; then
      count=$(rg -o "$pattern" "$file" | wc -l | tr -d ' ' || true)
      total=$((total + count))
    fi
  done
  echo "$total"
}

for pattern in 'unwrap\(' 'expect\(' '\bunsafe\b' 'println!\(' 'eprintln!\(' 'panic!\(' 'todo!\(' 'unimplemented!\('; do
  base_count="$(count_base_matches "$pattern")"
  head_count="$(count_head_matches "$pattern")"
  if (( head_count > base_count )); then
    echo "pattern '$pattern' increased in T-R2 Step2 scope: $base_count -> $head_count" >&2
    exit 1
  fi
done

cargo fmt --all --check
cargo clippy -q -p assay-core --all-targets -- -D warnings
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::emission::test_handler_emits_decision_on_policy_allow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::delegation::delegated_context_emits_typed_fields_for_supported_flow' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::approval::approval_required_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::scope::restrict_scope_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::redaction::redact_args_target_missing_denies' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::emission::test_tool_drift_deny_emits_alert_obligation_outcome' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::classification::test_operation_class_for_tool' -- --exact
cargo test -q -p assay-core --lib 'mcp::tool_call_handler::tests::lifecycle::test_lifecycle_emitter_not_called_when_none' -- --exact

echo "[review] PASS"
