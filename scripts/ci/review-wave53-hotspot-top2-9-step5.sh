#!/usr/bin/env bash
set -euo pipefail

export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"

base_ref="${BASE_REF:-origin/codex/wave53-hotspot-top2-9-step4}"
if ! git rev-parse --verify "$base_ref" >/dev/null 2>&1; then
  if [[ -z "${BASE_REF:-}" ]] && git rev-parse --verify codex/wave53-hotspot-top2-9-step4 >/dev/null 2>&1; then
    base_ref="codex/wave53-hotspot-top2-9-step4"
  else
    echo "FAIL: cannot resolve Step5 base ref: $base_ref"
    echo "Set BASE_REF to the Step4 branch/ref used for this stacked review."
    exit 1
  fi
fi

base_changed="$(git diff --name-only "$base_ref"...HEAD)"
worktree_changed="$(
  {
    git diff --name-only
    git diff --cached --name-only
    git ls-files --others --exclude-standard
  } | sort -u
)"
changed="$(printf '%s\n%s\n' "$base_changed" "$worktree_changed" | sed '/^$/d' | sort -u)"

allowed_pattern='^(docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9\.md|docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step5\.md|docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step5\.md|docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step5\.md|scripts/ci/review-wave53-hotspot-top2-9-step5\.sh|crates/assay-core/src/mcp/policy/mod\.rs|crates/assay-core/src/mcp/policy/types\.rs|crates/assay-core/src/mcp/policy/deserialize\.rs|crates/assay-core/src/mcp/policy/matcher\.rs|crates/assay-core/src/mcp/policy/contracts\.rs)$'
unexpected="$(printf '%s\n' "$changed" | rg -v "$allowed_pattern" || true)"
if [[ -n "$unexpected" ]]; then
  echo "FAIL: Wave53 Step5 changed files outside the allowlist:"
  printf '%s\n' "$unexpected"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^\.github/workflows/' >/dev/null; then
  echo "FAIL: workflow edits are out of scope for Wave53 Step5"
  exit 1
fi

if printf '%s\n' "$changed" | rg '^crates/assay-core/src/mcp/policy/engine_next/' >/dev/null; then
  echo "FAIL: policy/engine_next edits are out of scope for Wave53 Step5"
  exit 1
fi

required=(
  "docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md"
  "docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step5.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step5.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step5.md"
  "scripts/ci/review-wave53-hotspot-top2-9-step5.sh"
  "crates/assay-core/src/mcp/policy/mod.rs"
  "crates/assay-core/src/mcp/policy/types.rs"
  "crates/assay-core/src/mcp/policy/deserialize.rs"
  "crates/assay-core/src/mcp/policy/matcher.rs"
  "crates/assay-core/src/mcp/policy/contracts.rs"
)

for path in "${required[@]}"; do
  test -f "$path" || {
    echo "FAIL: missing required file: $path"
    exit 1
  }
done

require_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if ! rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

forbid_marker() {
  local pattern="$1"
  local path="$2"
  local message="$3"
  if rg -q "$pattern" "$path"; then
    echo "FAIL: $message"
    exit 1
  fi
}

for module in contracts deserialize engine engine_next legacy matcher response schema types; do
  require_marker "^mod ${module};$" "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must declare ${module} module"
done
require_marker '^pub use contracts::PolicyDecisionContract;$' "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must re-export PolicyDecisionContract"
require_marker '^pub use types::\*;$' "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must re-export moved policy types"
require_marker '^pub\(in crate::mcp::policy\) use matcher::matches_tool_pattern;$' "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must expose matcher inside policy module"
require_marker '^impl McpPolicy\b' "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must keep McpPolicy inherent methods"
forbid_marker '^pub struct McpPolicy\b|^pub enum PolicyDecision\b|^fn deserialize_constraints\b|^fn matches_tool_pattern\b|^pub struct PolicyDecisionContract\b' "crates/assay-core/src/mcp/policy/mod.rs" "policy facade must not own moved type/helper definitions"

require_marker '^pub struct McpPolicy\b' "crates/assay-core/src/mcp/policy/types.rs" "types module must own McpPolicy"
require_marker '^pub enum PolicyDecision\b' "crates/assay-core/src/mcp/policy/types.rs" "types module must own PolicyDecision"
require_marker 'super::deserialize::deserialize_constraints' "crates/assay-core/src/mcp/policy/types.rs" "types module must keep legacy constraints deserializer wiring"
require_marker '^pub use super::super::runtime_features::' "crates/assay-core/src/mcp/policy/types.rs" "types module must re-export runtime feature policy types"
require_marker '^pub\(super\) fn deserialize_constraints' "crates/assay-core/src/mcp/policy/deserialize.rs" "deserialize module must own constraints compatibility helper"
require_marker '^pub\(in crate::mcp::policy\) fn matches_tool_pattern' "crates/assay-core/src/mcp/policy/matcher.rs" "matcher module must own tool pattern matching"
require_marker '^pub struct PolicyDecisionContract\b' "crates/assay-core/src/mcp/policy/contracts.rs" "contracts module must own PolicyDecisionContract"
require_marker '^impl PolicyDecision\b' "crates/assay-core/src/mcp/policy/contracts.rs" "contracts module must own typed_contract implementation"
require_marker '^fn is_alert_deny_code\b' "crates/assay-core/src/mcp/policy/contracts.rs" "contracts module must preserve alert deny mapping helper"

check_loc_max() {
  local path="$1"
  local max="$2"
  local loc
  loc="$(wc -l < "$path" | tr -d ' ')"
  if (( loc > max )); then
    echo "FAIL: $path has $loc LOC, expected <= $max"
    exit 1
  fi
}

check_loc_max "crates/assay-core/src/mcp/policy/mod.rs" 140
check_loc_max "crates/assay-core/src/mcp/policy/types.rs" 360
check_loc_max "crates/assay-core/src/mcp/policy/contracts.rs" 220

cargo fmt --check
cargo check -p assay-core
cargo test -q -p assay-core --test policy_engine_test
cargo test -q -p assay-core --lib policy
cargo clippy -p assay-core --all-targets -- -D warnings
git diff --check

echo "PASS: Wave53 Step5 policy facade closure gate"
