#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"

base_ref="${BASE_REF:-${1:-}}"
if [[ -z "$base_ref" ]]; then
  if [[ -n "${GITHUB_BASE_REF:-}" ]]; then
    base_ref="origin/${GITHUB_BASE_REF}"
  else
    base_ref="origin/main"
  fi
fi

if ! git rev-parse --verify --quiet "${base_ref}^{commit}" >/dev/null; then
  echo "BASE_REF not found: ${base_ref}" >&2
  exit 1
fi

rg_bin="$(command -v rg || true)"
if [[ -z "$rg_bin" ]]; then
  echo "rg is required for reviewer anchors" >&2
  exit 1
fi

policy_mod="crates/assay-core/src/mcp/policy/mod.rs"
policy_engine="crates/assay-core/src/mcp/policy/engine.rs"
emit_file="crates/assay-core/src/mcp/tool_call_handler/emit.rs"
decision_file="crates/assay-core/src/mcp/decision.rs"
proxy_file="crates/assay-core/src/mcp/proxy.rs"
tool_tests="crates/assay-core/src/mcp/tool_call_handler/tests.rs"
decision_invariant="crates/assay-core/tests/decision_emit_invariant.rs"
evidence_types="crates/assay-evidence/src/types.rs"
adr_doc="docs/architecture/ADR-006-Evidence-Contract.md"
c1_doc="docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md"

require_fixed_pattern() {
  local pattern="$1"
  local file="$2"
  "$rg_bin" -n -F "$pattern" "$file" >/dev/null
}

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-core/src/mcp/policy/mod.rs|\
    crates/assay-core/src/mcp/policy/engine.rs|\
    crates/assay-core/src/mcp/tool_call_handler/emit.rs|\
    crates/assay-core/src/mcp/decision.rs|\
    crates/assay-core/src/mcp/proxy.rs|\
    crates/assay-core/src/mcp/tool_call_handler/tests.rs|\
    crates/assay-core/tests/decision_emit_invariant.rs|\
    crates/assay-evidence/src/types.rs|\
    docs/architecture/ADR-006-Evidence-Contract.md|\
    docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md|\
    docs/contributing/SPLIT-INVENTORY-wave-g2-delegation-context-signal-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-g2-delegation-context-signal-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-g2-delegation-context-signal-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-g2-delegation-context-signal-step1.md|\
    scripts/ci/review-wave-g2-delegation-context-signal-step1.sh)
      ;;
    crates/assay-core/src/*|\
    crates/assay-core/tests/*|\
    crates/assay-evidence/src/*|\
    docs/architecture/*|\
    docs/security/*)
      echo "ERROR: forbidden file changed for G2 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

require_fixed_pattern 'pub delegated_from: Option<String>' "$policy_mod"
require_fixed_pattern 'pub delegation_depth: Option<u32>' "$policy_mod"
require_fixed_pattern 'pub(super) delegated_from: Option<String>' "$emit_file"
require_fixed_pattern 'pub(super) delegation_depth: Option<u32>' "$emit_file"
require_fixed_pattern 'pub delegated_from: Option<String>' "$decision_file"
require_fixed_pattern 'pub delegation_depth: Option<u32>' "$decision_file"
require_fixed_pattern 'event.data.delegated_from = metadata.delegated_from.clone();' "$proxy_file"
require_fixed_pattern 'event.data.delegation_depth = metadata.delegation_depth;' "$proxy_file"
require_fixed_pattern 'pub delegated_from: Option<String>' "$evidence_types"
require_fixed_pattern 'pub delegation_depth: Option<u32>' "$evidence_types"
"$rg_bin" -n 'fn parse_delegation_context\(' "$policy_engine" >/dev/null
require_fixed_pattern '"delegation"' "$policy_engine"
require_fixed_pattern '"delegation"' "$tool_tests"
require_fixed_pattern '"delegation"' "$decision_invariant"

for required_test in \
  'fn delegated_context_emits_typed_fields_for_supported_flow(' \
  'fn direct_authorization_flow_omits_delegation_fields(' \
  'fn unstructured_delegation_hints_do_not_emit_typed_fields(' \
  'fn test_delegation_fields_are_additive_on_emitted_decisions(' \
  'fn parse_delegation_context_requires_explicit_delegated_from(' \
  'fn tool_decision_payload_delegation_fields_are_additive('; do
  "$rg_bin" -n -F "$required_test" "$tool_tests" "$decision_invariant" "$policy_engine" "$evidence_types" >/dev/null
done

if "$rg_bin" -n 'assay\.delegation' "$policy_mod" "$policy_engine" "$emit_file" "$decision_file" "$proxy_file" "$tool_tests" "$decision_invariant" >/dev/null; then
  echo "ERROR: G2 must not add a new delegation event type" >&2
  exit 1
fi

if "$rg_bin" -n 'actor_chain|inherited_scopes' "$policy_mod" "$policy_engine" "$emit_file" "$decision_file" "$proxy_file" "$tool_tests" "$decision_invariant" >/dev/null; then
  echo "ERROR: G2 must stay on the minimal delegation subset" >&2
  exit 1
fi

lower_docs="$(
  cat "$adr_doc" "$c1_doc" \
    | tr '[:upper:]' '[:lower:]'
)"

for forbidden_phrase in \
  "verifies delegation" \
  "proves privilege inheritance correctness" \
  "guarantees delegation chain integrity" \
  "validates delegated scopes" \
  "cryptographically verified delegation"; do
  if grep -Fq "$forbidden_phrase" <<<"$lower_docs"; then
    echo "ERROR: overclaim phrase present in G2 docs: $forbidden_phrase" >&2
    exit 1
  fi
done

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/packs/*|packs/open/*/pack.yaml)
      echo "ERROR: G2 must not change shipped pack YAMLs: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

cargo fmt --check
cargo clippy -q -p assay-core -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-core parse_delegation_context_
cargo test -q -p assay-core delegated_context_emits_typed_fields_for_supported_flow
cargo test -q -p assay-core direct_authorization_flow_omits_delegation_fields
cargo test -q -p assay-core unstructured_delegation_hints_do_not_emit_typed_fields
cargo test -q -p assay-core --test decision_emit_invariant test_delegation_fields_are_additive_on_emitted_decisions -- --exact
cargo test -q -p assay-evidence tool_decision_payload_delegation_fields_are_additive
git diff --check

echo "Wave G2 Step1 reviewer script: PASS"
