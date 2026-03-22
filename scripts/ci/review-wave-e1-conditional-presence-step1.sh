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

schema_file="crates/assay-evidence/src/lint/packs/schema.rs"
checks_file="crates/assay-evidence/src/lint/packs/checks.rs"
mandate_pack="crates/assay-evidence/packs/mandate-baseline.yaml"
conditional_test="crates/assay-evidence/tests/pack_engine_conditional_test.rs"
c1_test="crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs"
c1_fixture="crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml"
c1_doc="docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md"
mandate_spec="docs/architecture/SPEC-Mandate-v1.md"
mandate_adr="docs/architecture/ADR-017-Mandate-Evidence.md"
pack_spec="docs/architecture/SPEC-Pack-Engine-v1.md"

while IFS= read -r file; do
  [[ -n "$file" ]] || continue
  case "$file" in
    crates/assay-evidence/src/lint/packs/schema.rs|\
    crates/assay-evidence/src/lint/packs/checks.rs|\
    crates/assay-evidence/packs/mandate-baseline.yaml|\
    crates/assay-evidence/tests/pack_engine_conditional_test.rs|\
    crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs|\
    crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml|\
    docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md|\
    docs/architecture/SPEC-Mandate-v1.md|\
    docs/architecture/ADR-017-Mandate-Evidence.md|\
    docs/architecture/SPEC-Pack-Engine-v1.md|\
    docs/contributing/SPLIT-INVENTORY-wave-e1-conditional-presence-step1.md|\
    docs/contributing/SPLIT-CHECKLIST-wave-e1-conditional-presence-step1.md|\
    docs/contributing/SPLIT-MOVE-MAP-wave-e1-conditional-presence-step1.md|\
    docs/contributing/SPLIT-REVIEW-PACK-wave-e1-conditional-presence-step1.md|\
    scripts/ci/review-wave-e1-conditional-presence-step1.sh)
      ;;
    crates/assay-evidence/src/lint/packs/*|\
    crates/assay-evidence/packs/*|\
    crates/assay-evidence/tests/*|\
    docs/security/*|\
    docs/architecture/*)
      echo "ERROR: forbidden file changed for E1 scope: $file" >&2
      exit 1
      ;;
    *)
      echo "ERROR: out-of-scope file changed: $file" >&2
      exit 1
      ;;
  esac
done < <(git diff --name-only "${base_ref}...HEAD")

"$rg_bin" -n 'pub const ENGINE_VERSION: &str = "1\.1";' "$checks_file" >/dev/null
"$rg_bin" -n 'pub fn supported_conditional\(&self\) -> Result<SupportedConditionalCheck, String>' "$schema_file" >/dev/null
"$rg_bin" -n 'conditional then json_path_exists must contain exactly one required path' "$schema_file" >/dev/null
"$rg_bin" -n 'Unsupported conditional shape for engine v1\.1' "$checks_file" >/dev/null
"$rg_bin" -n 'fn scoped_events<' "$checks_file" >/dev/null

"$rg_bin" -n '^version: "1\.1\.0"$' "$mandate_pack" >/dev/null
"$rg_bin" -n '^    description: Allow tool decisions must include mandate context$' "$mandate_pack" >/dev/null
"$rg_bin" -n '^    engine_min_version: "1\.2"$' "$mandate_pack" >/dev/null

count_12="$(grep -Fc '    engine_min_version: "1.2"' "$mandate_pack")"
[[ "$count_12" == "4" ]] || {
  echo "ERROR: expected exactly 4 future mandate rules gated to v1.2, found $count_12" >&2
  exit 1
}

"$rg_bin" -n 'conditional-presence subset' "$mandate_spec" >/dev/null
"$rg_bin" -n 'Engine `v1\.1` supports only the narrow conditional-presence subset' "$mandate_spec" >/dev/null
"$rg_bin" -n 'MANDATE-001.*carry mandate context on the same event' "$mandate_adr" >/dev/null
"$rg_bin" -n '^#### `conditional` \(v1\.1 conditional-presence subset\)$' "$pack_spec" >/dev/null

"$rg_bin" -n '`A3-002` Allow decisions must carry mandate context' "$c1_doc" >/dev/null
"$rg_bin" -n 'Engine `1\.1` can execute this narrow conditional-presence form' "$c1_doc" >/dev/null

"$rg_bin" -n '^    description: allow decisions should require mandate context$' "$c1_fixture" >/dev/null

for required_test in \
  'fn conditional_rule_passes_when_no_events_match_condition(' \
  'fn conditional_rule_fails_when_matching_event_lacks_required_path(' \
  'fn adding_unrelated_non_matching_events_does_not_change_conditional_result(' \
  'fn event_field_present_respects_event_types_filter(' \
  'fn json_path_exists_respects_event_types_filter(' \
  'fn unsupported_conditional_shape_still_skips_for_security_pack(' \
  'fn unsupported_conditional_shape_fails_for_compliance_pack(' \
  'fn mandate_001_fails_allow_decision_without_mandate_id(' \
  'fn future_mandate_rules_remain_version_gated('; do
  "$rg_bin" -n -F "$required_test" "$conditional_test" >/dev/null
done

for required_test in \
  'fn a3_conditional_presence_rule_is_supported_in_engine_v1_1(' \
  'fn a3_conditional_presence_rule_fails_without_mandate_context(' \
  'fn a3_conditional_presence_rule_passes_with_mandate_context('; do
  "$rg_bin" -n -F "$required_test" "$c1_test" >/dev/null
done

lower_docs="$(
  cat "$c1_doc" "$mandate_spec" "$mandate_adr" "$pack_spec" "$mandate_pack" \
    | tr '[:upper:]' '[:lower:]'
)"

for forbidden_phrase in \
  "reference integrity is implemented" \
  "temporal validity is implemented" \
  "multi-event linkage is implemented" \
  "general policy language" \
  "arbitrary joins"; do
  if grep -Fq "$forbidden_phrase" <<<"$lower_docs"; then
    echo "ERROR: overclaim phrase present in E1 docs/pack: $forbidden_phrase" >&2
    exit 1
  fi
done

cargo fmt --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo test -q -p assay-evidence --test pack_engine_conditional_test
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping
cargo test -q -p assay-evidence --test pack_engine_manual_test
git diff --check

echo "Wave E1 Step1 reviewer script: PASS"
