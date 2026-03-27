#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

git rev-parse --verify "$BASE_REF" >/dev/null

changed_files() {
  {
    git diff --name-only "$BASE_REF"...HEAD
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | awk 'NF' | sort -u
}

ALLOWLIST=(
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/policy/engine.rs"
  "crates/assay-core/src/mcp/policy/engine_next/mod.rs"
  "crates/assay-core/src/mcp/policy/engine_next/matcher.rs"
  "crates/assay-core/src/mcp/policy/engine_next/effects.rs"
  "crates/assay-core/src/mcp/policy/engine_next/precedence.rs"
  "crates/assay-core/src/mcp/policy/engine_next/fail_closed.rs"
  "crates/assay-core/src/mcp/policy/engine_next/diagnostics.rs"
  "docs/contributing/SPLIT-PLAN-wave45-policy-engine.md"
  "docs/contributing/SPLIT-CHECKLIST-wave45-policy-engine-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave45-policy-engine-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave45-policy-engine-step2.md"
  "scripts/ci/review-wave45-policy-engine-step2.sh"
)

echo "[review] allowlist-only diff vs $BASE_REF (workflow-ban, tests ban)"
while IFS= read -r f; do
  [[ -z "${f:-}" ]] && continue

  if [[ "$f" == .github/workflows/* ]]; then
    echo "FAIL: Wave45 Step2 must not touch workflows ($f)"
    exit 1
  fi

  ok="false"
  for a in "${ALLOWLIST[@]}"; do
    [[ "$f" == "$a" ]] && ok="true" && break
  done

  if [[ "$ok" != "true" ]]; then
    echo "FAIL: file not allowed in Wave45 Step2: $f"
    exit 1
  fi
done < <(changed_files)

if changed_files | rg -n '^crates/assay-core/tests/' >/dev/null; then
  echo "FAIL: Wave45 Step2 must not change crates/assay-core/tests/**"
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/tests/** are not allowed in Wave45 Step2"
  git ls-files --others --exclude-standard -- 'crates/assay-core/tests/**' | sed 's/^/  - /'
  exit 1
fi

if git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/policy/**' | rg -n '.' >/dev/null; then
  echo "FAIL: untracked files under crates/assay-core/src/mcp/policy/** are not allowed in Wave45 Step2"
  git ls-files --others --exclude-standard -- 'crates/assay-core/src/mcp/policy/**' | sed 's/^/  - /'
  exit 1
fi

echo "[review] facade/module boundary checks"
[[ -f crates/assay-core/src/mcp/policy/engine_next/mod.rs ]] || {
  echo "FAIL: missing engine_next/mod.rs"
  exit 1
}
for module in matcher effects precedence fail_closed diagnostics; do
  [[ -f "crates/assay-core/src/mcp/policy/engine_next/${module}.rs" ]] || {
    echo "FAIL: missing engine_next/${module}.rs"
    exit 1
  }
done

rg -n '^mod engine_next;$' crates/assay-core/src/mcp/policy/mod.rs >/dev/null || {
  echo "FAIL: policy/mod.rs must wire engine_next"
  exit 1
}
rg -n '^pub\(super\) fn evaluate_with_metadata\(' crates/assay-core/src/mcp/policy/engine.rs >/dev/null || {
  echo "FAIL: engine.rs must keep evaluate_with_metadata(...)"
  exit 1
}
rg -n '^pub\(super\) fn check\(' crates/assay-core/src/mcp/policy/engine.rs >/dev/null || {
  echo "FAIL: engine.rs must keep check(...)"
  exit 1
}

for pattern in \
  '^fn check_rate_limits' \
  '^fn finalize_evaluation' \
  '^fn apply_approval_required_obligation' \
  '^fn apply_restrict_scope_obligation' \
  '^fn apply_redact_args_obligation' \
  '^fn is_denied' \
  '^fn has_allowlist' \
  '^fn is_allowed' \
  '^fn format_deny_contract' \
  '^fn parse_delegation_context'; do
  if rg -n "$pattern" crates/assay-core/src/mcp/policy/engine.rs >/dev/null; then
    echo "FAIL: engine.rs still contains extracted helper pattern: $pattern"
    exit 1
  fi
done

engine_loc="$(wc -l < crates/assay-core/src/mcp/policy/engine.rs | tr -d '[:space:]')"
if [[ "${engine_loc}" -gt 320 ]]; then
  echo "FAIL: engine.rs facade budget exceeded (${engine_loc} > 320)"
  exit 1
fi

echo "[review] gates"
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings

cargo test -q -p assay-core --test policy_engine_test test_mixed_tools_config -- --exact
cargo test -q -p assay-core --test policy_engine_test test_constraint_enforcement -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_policy_file_blocks_alt_sink_by_class -- --exact
cargo test -q -p assay-core --test tool_taxonomy_policy_match tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -q -p assay-core --test decision_emit_invariant approval::approval_required_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant restrict_scope::restrict_scope_target_missing_denies -- --exact
cargo test -q -p assay-core --test decision_emit_invariant redaction::redact_args_target_missing_denies -- --exact
cargo test -q -p assay-core --lib 'mcp::policy::engine::tests::parse_delegation_context_uses_explicit_depth_only' -- --exact

echo "[review] PASS"
