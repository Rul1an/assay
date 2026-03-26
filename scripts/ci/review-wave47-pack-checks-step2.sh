#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-evidence/src/lint/packs/checks.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/mod.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/event.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/json_path.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/conditional.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/manifest.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/finding.rs"
  "docs/contributing/SPLIT-PLAN-wave47-pack-checks.md"
  "docs/contributing/SPLIT-CHECKLIST-wave47-pack-checks-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave47-pack-checks-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave47-pack-checks-step2.md"
  "scripts/ci/review-wave47-pack-checks-step2.sh"
)

DIFF_FILES=()
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git diff --name-only "$BASE_REF" --)
while IFS= read -r file; do
  DIFF_FILES+=("$file")
done < <(git ls-files --others --exclude-standard)

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
    [[ -z "$file" ]] && continue
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
  done
fi

if (( ${#DIFF_FILES[@]} > 0 )); then
  for file in "${DIFF_FILES[@]}"; do
    [[ -z "$file" ]] && continue
    if [[ "$file" == crates/assay-evidence/tests/* ]]; then
      echo "external assay-evidence tests must remain untouched in Step2" >&2
      exit 1
    fi
    if [[ "$file" == packs/open/* ]]; then
      echo "open pack mirrors must remain untouched in Step2" >&2
      exit 1
    fi
    if [[ "$file" == crates/assay-evidence/src/lint/packs/schema.rs ]]; then
      echo "schema.rs must remain untouched in Wave47 Step2" >&2
      exit 1
    fi
    if [[ "$file" == crates/assay-evidence/src/lint/packs/schema_next/* ]]; then
      echo "schema_next/* must remain untouched in Wave47 Step2" >&2
      exit 1
    fi
  done
fi

if ! rg -n '^#\[path = "checks_next/mod.rs"\]$' crates/assay-evidence/src/lint/packs/checks.rs >/dev/null; then
  echo "checks.rs must declare the sibling checks_next path override" >&2
  exit 1
fi

if ! rg -n '^mod checks_next;$' crates/assay-evidence/src/lint/packs/checks.rs >/dev/null; then
  echo "checks.rs must declare checks_next module" >&2
  exit 1
fi

for forbidden in \
  '^fn check_g3_authorization_context_present\(' \
  '^fn check_json_path_exists\(' \
  '^fn check_event_count\(' \
  '^fn check_event_pairs\(' \
  '^fn check_event_field_present\(' \
  '^fn check_event_type_exists\(' \
  '^fn check_conditional\(' \
  '^fn check_manifest_field\(' \
  '^fn create_finding\(' \
  '^fn create_finding_with_severity\(' \
  '^fn compile_glob\(' \
  '^fn scoped_events\(' \
  '^fn value_pointer\(' \
  '^trait LintFindingExt'
do
  if rg -n "$forbidden" crates/assay-evidence/src/lint/packs/checks.rs >/dev/null; then
    echo "checks.rs still contains extracted implementation symbol: $forbidden" >&2
    exit 1
  fi
done

RUST_SCOPE_FILES=(
  "crates/assay-evidence/src/lint/packs/checks.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/mod.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/event.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/json_path.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/conditional.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/manifest.rs"
  "crates/assay-evidence/src/lint/packs/checks_next/finding.rs"
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
    echo "pattern '$pattern' increased in checks split scope: $base_count -> $head_count" >&2
    exit 1
  fi
done

cargo fmt --all --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo check -q -p assay-evidence

cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::g3_authorization_check_uses_scoped_events_not_full_bundle' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_value_pointer' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::checks::tests::test_glob_matching' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test event_field_present_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_verified_and_pack_passes -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp001_aligns_trust_basis_absent_and_pack_fails -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_dc_001_fails_when_agent_card_visible_is_string_not_bool -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_is_supported_in_engine_v1_1 -- --exact
cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping a3_conditional_presence_rule_fails_without_mandate_context -- --exact

echo "[review] PASS"
