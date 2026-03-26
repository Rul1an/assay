#!/usr/bin/env bash
set -euo pipefail

BASE_REF="${BASE_REF:-origin/main}"
ROOT="$(git rev-parse --show-toplevel)"
cd "$ROOT"

ALLOWED_FILES=(
  "crates/assay-evidence/src/lint/packs/schema.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/mod.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/types.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/serde.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/validation.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/conditional.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/errors.rs"
  "docs/contributing/SPLIT-PLAN-wave46-pack-schema.md"
  "docs/contributing/SPLIT-CHECKLIST-wave46-pack-schema-step2.md"
  "docs/contributing/SPLIT-MOVE-MAP-wave46-pack-schema-step2.md"
  "docs/contributing/SPLIT-REVIEW-PACK-wave46-pack-schema-step2.md"
  "scripts/ci/review-wave46-pack-schema-step2.sh"
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
  done
fi

if ! rg -n '^mod schema_next;$' crates/assay-evidence/src/lint/packs/schema.rs >/dev/null; then
  echo "schema.rs must declare schema_next module" >&2
  exit 1
fi

if ! rg -n '^pub use schema_next::' crates/assay-evidence/src/lint/packs/schema.rs >/dev/null; then
  echo "schema.rs must re-export schema_next facade" >&2
  exit 1
fi

for forbidden in 'pub enum PackKind' 'pub struct PackDefinition' 'pub struct PackRule' 'pub enum CheckDefinition' 'pub enum PackValidationError' 'struct RawConditionalCondition'; do
  if rg -n "$forbidden" crates/assay-evidence/src/lint/packs/schema.rs >/dev/null; then
    echo "schema.rs still contains extracted implementation symbol: $forbidden" >&2
    exit 1
  fi
done

RUST_SCOPE_FILES=(
  "crates/assay-evidence/src/lint/packs/schema.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/mod.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/types.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/serde.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/validation.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/conditional.rs"
  "crates/assay-evidence/src/lint/packs/schema_next/errors.rs"
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
    echo "pattern '$pattern' increased in schema split scope: $base_count -> $head_count" >&2
    exit 1
  fi
done

cargo fmt --all --check
cargo clippy -q -p assay-evidence --all-targets -- -D warnings
cargo check -q -p assay-evidence

cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_is_valid_pack_name' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_builtin_wins_over_local' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_local_invalid_yaml_fails' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::loader::loader_internal::tests::test_path_wins_over_builtin' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_supported_conditional_shape_parses' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_with_multiple_then_paths_is_unsupported' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_condition_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_conditional_validation_requires_then_object' -- --exact
cargo test -q -p assay-evidence --lib 'lint::packs::schema::tests::test_json_path_exists_value_equals_requires_exactly_one_path' -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test conditional_rule_fails_when_matching_event_lacks_required_path -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test json_path_exists_respects_event_types_filter -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_still_skips_for_security_pack -- --exact
cargo test -q -p assay-evidence --test pack_engine_conditional_test unsupported_conditional_shape_fails_for_compliance_pack -- --exact
cargo test -q -p assay-evidence --test a2a_discovery_card_followup_pack a2a_discovery_builtin_and_open_pack_are_exactly_equivalent -- --exact
cargo test -q -p assay-evidence --test mcp_signal_followup_pack mcp_followup_builtin_and_open_pack_are_exactly_equivalent -- --exact
