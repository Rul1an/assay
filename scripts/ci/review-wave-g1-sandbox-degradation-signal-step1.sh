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

types_file="crates/assay-evidence/src/types.rs"
events_file="crates/assay-cli/src/profile/events.rs"
profile_mod="crates/assay-cli/src/profile/mod.rs"
profile_types="crates/assay-cli/src/cli/commands/profile_types.rs"
mapping_file="crates/assay-cli/src/cli/commands/evidence/mapping.rs"
evidence_cmd="crates/assay-cli/src/cli/commands/evidence/mod.rs"
sandbox_cmd="crates/assay-cli/src/cli/commands/sandbox.rs"
evidence_test="crates/assay-cli/tests/evidence_test.rs"
profile_test="crates/assay-cli/tests/profile_integration_test.rs"
c1_test="crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs"
adr_doc="docs/architecture/ADR-006-Evidence-Contract.md"
metrics_doc="docs/architecture/evidence-metrics-mapping.md"
c1_doc="docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md"
sandbox_doc="docs/reference/cli/sandbox.md"
traces_doc="docs/concepts/traces.md"

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/src/types.rs|\
    crates/assay-cli/src/profile/events.rs|\
    crates/assay-cli/src/profile/mod.rs|\
    crates/assay-cli/src/cli/commands/profile_types.rs|\
    crates/assay-cli/src/cli/commands/evidence/mapping.rs|\
    crates/assay-cli/src/cli/commands/evidence/mod.rs|\
    crates/assay-cli/src/cli/commands/sandbox.rs|\
    crates/assay-cli/tests/evidence_test.rs|\
    crates/assay-cli/tests/profile_integration_test.rs|\
    crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs|\
    docs/architecture/ADR-006-Evidence-Contract.md|\
    docs/architecture/evidence-metrics-mapping.md|\
    docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md|\
    docs/reference/cli/sandbox.md|\
    docs/concepts/traces.md|\
    docs/contributing/SPLIT-INVENTORY-wave-g1-sandbox-degradation-signal-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-g1-sandbox-degradation-signal-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-g1-sandbox-degradation-signal-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-g1-sandbox-degradation-signal-step1.md|\
    scripts/ci/review-wave-g1-sandbox-degradation-signal-step1.sh)
      ;;
    crates/assay-cli/src/*|\
    crates/assay-cli/tests/*|\
    crates/assay-evidence/src/*|\
    crates/assay-evidence/tests/*|\
    docs/architecture/*|\
    docs/security/*|\
    docs/reference/*|\
    docs/concepts/*)
      echo "ERROR: forbidden file changed for G1 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

"$rg_bin" -n 'pub enum SandboxDegradationReasonCode' "$types_file" >/dev/null
"$rg_bin" -n 'pub enum SandboxDegradationMode' "$types_file" >/dev/null
"$rg_bin" -n 'pub enum SandboxDegradationComponent' "$types_file" >/dev/null
"$rg_bin" -n 'pub struct PayloadSandboxDegraded' "$types_file" >/dev/null
"$rg_bin" -n 'pub detail: Option<String>' "$types_file" >/dev/null

"$rg_bin" -n 'SandboxDegraded \{' "$events_file" >/dev/null
"$rg_bin" -n 'payload: serde_json::from_value' "$events_file" >/dev/null
"$rg_bin" -n 'pub sandbox_degradations: Vec<PayloadSandboxDegraded>' "$profile_mod" >/dev/null
"$rg_bin" -n 'collector_suppresses_duplicate_sandbox_degradations_per_component_reason' "$profile_mod" >/dev/null
"$rg_bin" -n 'pub sandbox_degradations: Vec<PayloadSandboxDegraded>' "$profile_types" >/dev/null

"$rg_bin" -n 'fn backend_unavailable_degradation\(' "$sandbox_cmd" >/dev/null
"$rg_bin" -n 'fn policy_conflict_degradation\(' "$sandbox_cmd" >/dev/null
"$rg_bin" -n 'fn evidence_profile_path\(' "$sandbox_cmd" >/dev/null
"$rg_bin" -n 'fn evidence_profile_run_id\(' "$sandbox_cmd" >/dev/null
"$rg_bin" -n 'Evidence Profile:' "$sandbox_cmd" >/dev/null

"$rg_bin" -n '"assay\.sandbox\.degraded"' "$mapping_file" >/dev/null
"$rg_bin" -n 'sandbox_degradation_count' "$mapping_file" >/dev/null
"$rg_bin" -n 'sandbox evidence sidecar' "$evidence_cmd" >/dev/null

for required_test in \
  'fn test_evidence_export_includes_sandbox_degraded_event_when_profile_contains_degradation(' \
  'fn test_profile_cli_workflow(' \
  'fn backend_unavailable_emits_degradation_when_enforcement_requested_and_run_continues(' \
  'fn intentional_permissive_mode_does_not_emit_degradation(' \
  'fn fail_closed_policy_conflict_does_not_emit_degradation(' \
  'fn a5_sandbox_signal_exists_for_supported_degraded_flow('; do
  "$rg_bin" -n -F "$required_test" "$evidence_test" "$profile_test" "$sandbox_cmd" "$c1_test" >/dev/null
done

"$rg_bin" -n 'weaker-than-requested' "$c1_doc" "$sandbox_doc" >/dev/null
"$rg_bin" -n 'execution continued' "$c1_doc" "$sandbox_doc" >/dev/null
"$rg_bin" -n 'no longer a pure signal gap' "$c1_doc" >/dev/null
"$rg_bin" -n 'out\.evidence\.yaml' "$traces_doc" >/dev/null
"$rg_bin" -n 'degradation_mode' "$adr_doc" >/dev/null
"$rg_bin" -n 'Supported weaker-than-requested containment fell back to audit while execution continued' "$metrics_doc" >/dev/null

lower_docs="$(
  cat "$adr_doc" "$metrics_doc" "$c1_doc" "$sandbox_doc" "$traces_doc" \
    | tr '[:upper:]' '[:lower:]'
)"

for forbidden_phrase in \
  "proves sandboxing" \
  "guarantees containment" \
  "all sandbox failures detected" \
  "general sandbox health" \
  "a5-002 solved"; do
  if grep -Fq "$forbidden_phrase" <<<"$lower_docs"; then
    echo "ERROR: overclaim phrase present in G1 docs: $forbidden_phrase" >&2
    exit 1
  fi
done

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/packs/*|packs/open/*)
      echo "ERROR: G1 must not change shipped pack YAMLs: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

cargo fmt --check
cargo clippy -q -p assay-cli -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --lib
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping
cargo test -q -p assay-cli --test evidence_test
cargo test -q -p assay-cli --test profile_integration_test
cargo test -q -p assay-cli backend_unavailable_emits_degradation_when_enforcement_requested_and_run_continues
cargo test -q -p assay-cli policy_conflict_emits_degradation_only_when_execution_continues
cargo test -q -p assay-cli intentional_permissive_mode_does_not_emit_degradation
cargo test -q -p assay-cli fail_closed_policy_conflict_does_not_emit_degradation
cargo test -q -p assay-cli collector_suppresses_duplicate_sandbox_degradations_per_component_reason
git diff --check

echo "Wave G1 Step1 reviewer script: PASS"
